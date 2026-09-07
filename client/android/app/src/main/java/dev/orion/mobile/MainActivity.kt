package dev.orion.mobile

import android.content.Intent
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import android.widget.Button
import android.widget.ImageButton
import android.widget.TextView
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.MultiFormatReader
import com.google.zxing.RGBLuminanceSource
import com.google.zxing.common.HybridBinarizer
import com.wireguard.android.backend.Tunnel
import com.wireguard.config.Config

class MainActivity : AppCompatActivity() {

    private val app get() = application as OrionApp

    private val pickQr = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri?.let { decodeQr(it) }
    }

    /** Camera scan via zxing-android-embedded; result carries the payload. */
    private val scanQr = registerForActivityResult(com.journeyapps.barcodescanner.ScanContract()) { result ->
        val contents = result.contents
        if (contents != null) {
            importConf(contents, autoConnect = false, persist = true)
            subline.text = "Profile imported from QR - press CONNECT."
        }
        // null contents = user cancelled; leave state as it was
    }

    private lateinit var stateWord: TextView
    private lateinit var subline: TextView
    private lateinit var powerBtn: ImageButton
    private lateinit var powerBtn2: Button
    private lateinit var ringView: RingView
    private lateinit var rxView: TextView
    private lateinit var txView: TextView
    private lateinit var hsView: TextView
    private lateinit var profileView: TextView
    private var config: Config? = null
    private var connectedSince: Long? = null
    private var connecting = false
    private var lastError: String? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        stateWord = findViewById(R.id.stateWord)
        subline = findViewById(R.id.subline)
        powerBtn = findViewById(R.id.powerBtn)
        powerBtn2 = findViewById(R.id.powerBtn2)
        ringView = findViewById(R.id.ringView)
        rxView = findViewById(R.id.rx)
        txView = findViewById(R.id.tx)
        hsView = findViewById(R.id.hs)
        profileView = findViewById(R.id.profileView)

        powerBtn.setOnClickListener { onPower() }
        powerBtn2.setOnClickListener { onPower() }
        findViewById<Button>(R.id.importBtn).setOnClickListener { askImportSource() }
        findViewById<Button>(R.id.alwaysOnBtn).setOnClickListener { openVpnSettings() }

        val viaIntent = intent?.getStringExtra("conf_b64") != null || intent?.getStringExtra("conf") != null
        if (viaIntent) importFromIntent(intent) else loadPersisted()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        importFromIntent(intent)
        render()
    }

    /** Profile arrives base64-encoded to survive adb/am shell quoting. */
    private fun importFromIntent(intent: Intent) {
        val b64 = intent.getStringExtra("conf_b64")
        val confText = if (b64 != null)
            String(android.util.Base64.decode(b64, android.util.Base64.DEFAULT))
        else
            intent.getStringExtra("conf")?.replace("\\n", "\n") ?: return
        importConf(confText, intent.getBooleanExtra("connect", false), persist = true)
    }

    private fun loadPersisted() {
        val text = app.persistedConf() ?: return
        importConf(text, autoConnect = false, persist = false)
    }

    /** Camera scan when the device has one; the image-file path stays for
     *  desktop-generated QR screenshots. */
    private fun askImportSource() {
        val hasCamera = packageManager.hasSystemFeature(android.content.pm.PackageManager.FEATURE_CAMERA_ANY)
        val options = buildList {
            if (hasCamera) add("Scan with camera")
            add("Choose an image file")
        }.toTypedArray()
        AlertDialog.Builder(this)
            .setTitle("Import profile QR")
            .setItems(options) { _, which ->
                val label = options[which]
                if (label == "Scan with camera") {
                    val opts = com.journeyapps.barcodescanner.ScanOptions().apply {
                        setDesiredBarcodeFormats(com.journeyapps.barcodescanner.ScanOptions.QR_CODE)
                        setPrompt("Point the camera at the Orion QR")
                        setBeepEnabled(false)
                        setOrientationLocked(true)
                    }
                    scanQr.launch(opts)
                } else {
                    pickQr.launch(arrayOf("image/*"))
                }
            }
            .setNegativeButton("Cancel", null)
            .show()
    }

    private fun importConf(confText: String, autoConnect: Boolean, persist: Boolean) {
        try {
            config = Config.parse(confText.byteInputStream())
            if (persist) app.persistConf(confText)
            val ep = config?.peers?.firstOrNull()?.endpoint?.map { it.toString() }?.orElse("?")
            profileView.text = "profile: ${config?.`interface`?.getAddresses()?.firstOrNull()} -> $ep"
            lastError = null
            subline.text = if (persist) "Profile saved on this device." else subline.text
            render()
            if (autoConnect) onPower()
        } catch (e: Exception) {
            lastError = "profile error: ${e.message}"
            subline.text = lastError
        }
    }

    private fun onPower() {
        val cfg = config ?: run {
            lastError = "No profile loaded - import a QR or pass the conf extra."
            render()
            return
        }
        if (connecting) return
        connecting = true
        render()
        Thread {
            val result: Pair<Boolean, String?> = try {
                if (app.backend.getState(OrionTunnel) == Tunnel.State.UP) {
                    app.backend.setState(OrionTunnel, Tunnel.State.DOWN, null)
                    Pair(true, null)
                } else {
                    app.backend.setState(OrionTunnel, Tunnel.State.UP, cfg)
                    Pair(true, null)
                }
            } catch (e: Exception) {
                Pair(false, e.message ?: e.javaClass.simpleName)
            }
            runOnUiThread {
                connecting = false
                if (result.second != null) lastError = "tunnel error: ${result.second}"
                if (up() && connectedSince == null) connectedSince = System.currentTimeMillis()
                if (!up()) connectedSince = null
                render()
                poll()
            }
        }.start()
    }

    private fun up(): Boolean = try {
        app.backend.getState(OrionTunnel) == Tunnel.State.UP
    } catch (e: Exception) {
        false
    }

    private fun render() {
        val secured = up()
        val p = when {
            secured -> "SECURED"
            connecting -> "LINKING"
            else -> "STANDBY"
        }
        stateWord.text = p
        stateChip(p)
        powerBtn.isEnabled = !connecting
        powerBtn2.isEnabled = !connecting
        powerBtn2.text = if (secured) "DISCONNECT" else "CONNECT"
        subline.text = lastError ?: when (p) {
            "SECURED" -> "Encrypted to your node; exits with its address."
            "LINKING" -> "Establishing the obfuscated tunnel."
            else -> "Traffic unconfined. Load a profile and connect."
        }
        if (secured) {
            val stats = app.backend.getStatistics(OrionTunnel)
            rxView.text = fmtBytes(stats.totalRx())
            txView.text = fmtBytes(stats.totalTx())
            hsView.text = "live"
        } else {
            rxView.text = "--"; txView.text = "--"; hsView.text = "--"
        }
        ringView.setSecured(secured)
        ringView.setTracking(connecting)
    }

    private fun stateChip(p: String) {
        findViewById<TextView>(R.id.stateChip).text = when (p) {
            "SECURED" -> "TUNNEL UP"
            "LINKING" -> "LINKING"
            else -> "OFFLINE"
        }
    }

    /** Decode a QR image picked from storage into a WireGuard config. */
    private fun decodeQr(uri: Uri) {
        try {
            val bmp = BitmapFactory.decodeStream(contentResolver.openInputStream(uri))
                ?: throw IllegalArgumentException("not an image")
            val w = bmp.width
            val h = bmp.height
            val pixels = IntArray(w * h)
            bmp.getPixels(pixels, 0, w, 0, 0, w, h)
            val result = MultiFormatReader().decode(
                BinaryBitmap(HybridBinarizer(RGBLuminanceSource(w, h, pixels))),
                mapOf(DecodeHintType.TRY_HARDER to true)
            )
            bmp.recycle()
            importConf(result.text, autoConnect = false, persist = true)
            subline.text = "Profile imported from QR - press CONNECT."
        } catch (e: Exception) {
            lastError = "QR decode failed: ${e.message ?: e.javaClass.simpleName}"
            render()
        }
    }

    /** System VPN page holds the Always-on + "block without VPN" toggles. */
    private fun openVpnSettings() {
        try {
            startActivity(Intent(Settings.ACTION_VPN_SETTINGS))
        } catch (e: Exception) {
            startActivity(Intent(Settings.ACTION_SETTINGS))
        }
    }

    /** Lightweight stats refresh while secured; stops itself when down. */
    private fun poll() {
        android.os.Handler(android.os.Looper.getMainLooper()).postDelayed({
            render()
            if (up()) poll()
        }, 1500)
    }

    private fun fmtBytes(n: Long): String {
        val u = listOf("B", "KiB", "MiB", "GiB")
        var i = 0; var v = n.toDouble()
        while (v >= 1024 && i < u.size - 1) { v /= 1024; i++ }
        return "${"%.1f".format(v)} ${u[i]}"
    }

    override fun onResume() {
        super.onResume()
        render()
        if (up()) poll()
    }
}
