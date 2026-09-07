package dev.orion.mobile

import android.content.Intent
import android.os.Bundle
import android.widget.Button
import android.widget.ImageButton
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import com.wireguard.android.backend.GoBackend
import com.wireguard.android.backend.Statistics
import com.wireguard.android.backend.Tunnel
import com.wireguard.config.Config

/** The library's Tunnel contract is just a name plus a state-change hook. */
private class OrionTunnel(private val n: String) : Tunnel {
    override fun getName(): String = n
    override fun onStateChange(newState: Tunnel.State) {}
}

class MainActivity : AppCompatActivity() {

    private lateinit var backend: GoBackend
    private var tunnel: OrionTunnel? = null
    private var config: Config? = null
    private var lastStats: Statistics? = null

    private lateinit var stateWord: TextView
    private lateinit var subline: TextView
    private lateinit var powerBtn: ImageButton
    private lateinit var powerBtn2: Button
    private lateinit var ringView: RingView
    private lateinit var rxView: TextView
    private lateinit var txView: TextView
    private lateinit var hsView: TextView
    private lateinit var profileView: TextView
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

        backend = GoBackend(applicationContext)

        powerBtn.setOnClickListener { onPower() }
        powerBtn2.setOnClickListener { onPower() }
        intent?.let { importFromIntent(it) }
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
        importConf(confText, intent.getBooleanExtra("connect", false))
    }

    private fun importConf(confText: String, autoConnect: Boolean) {
        try {
            config = Config.parse(confText.byteInputStream())
            tunnel = OrionTunnel("orion")
            val ep = config?.peers?.firstOrNull()?.endpoint?.map { it.toString() }?.orElse("?")
            profileView.text = "profile: ${config?.`interface`?.getAddresses()?.firstOrNull()} -> $ep"
            render()
            if (autoConnect) onPower()
        } catch (e: Exception) {
            lastError = "profile error: ${e.message}"
            subline.text = lastError
        }
    }

    private fun onPower() {
        val t = tunnel ?: run { subline.text = "No profile loaded - pass the conf intent extra."; return }
        val cfg = config ?: return
        if (connecting) return
        connecting = true
        render()
        Thread {
            val result: Pair<Boolean, String?> = try {
                if (backend.getState(t) == Tunnel.State.UP) {
                    backend.setState(t, Tunnel.State.DOWN, null)
                    Pair(true, null)
                } else {
                    backend.setState(t, Tunnel.State.UP, cfg)
                    Pair(true, null)
                }
            } catch (e: Exception) {
                Pair(false, e.message ?: e.javaClass.simpleName)
            }
            runOnUiThread {
                connecting = false
                if (result.second != null) subline.text = "tunnel error: ${result.second}"
                if (backend.getState(t) == Tunnel.State.UP && connectedSince == null) connectedSince = System.currentTimeMillis()
                if (backend.getState(t) == Tunnel.State.DOWN) connectedSince = null
                render()
                poll()
            }
        }.start()
    }

    private fun up(): Boolean = tunnel?.let { backend.getState(it) == Tunnel.State.UP } ?: false

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
            val t = tunnel
            val stats = t?.let { backend.getStatistics(it) }
            lastStats = stats
            rxView.text = stats?.let { fmtBytes(it.totalRx()) } ?: "--"
            txView.text = stats?.let { fmtBytes(it.totalTx()) } ?: "--"
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

    /** Lightweight stats refresh while secured; stops itself when down. */
    private fun poll() {
        android.os.Handler(android.os.Looper.getMainLooper()).postDelayed({
            if (tunnel != null) render()
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
