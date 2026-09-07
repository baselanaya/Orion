package dev.orion.mobile

import android.app.Application
import com.wireguard.android.backend.GoBackend
import com.wireguard.android.backend.Tunnel
import com.wireguard.config.Config

/**
 * One tunnel identity for the whole process. GoBackend tracks the active
 * tunnel by object identity, so the app, the always-on path and the boot
 * reconnect path must all use this same instance.
 */
object OrionTunnel : Tunnel {
    override fun getName(): String = "orion"
    override fun onStateChange(newState: Tunnel.State) {}
}

/**
 * Application-scoped backend + profile persistence.
 *
 * The profile is stored in app-private storage (filesDir, mode_private) so the
 * always-on boot path can bring the tunnel up without any UI. The system
 * starts the VpnService directly in always-on mode; GoBackend's callback then
 * runs in this process with no activity — hence the logic lives here.
 */
class OrionApp : Application() {
    lateinit var backend: GoBackend
        private set

    override fun onCreate() {
        super.onCreate()
        backend = GoBackend(this)
        GoBackend.setAlwaysOnCallback {
            // Network may not be settled yet at boot; retry through it.
            Thread { ensureUp(retries = 6) }.start()
        }
    }

    /** Bring the persisted tunnel up. Returns true once the state is UP. */
    fun ensureUp(retries: Int = 1): Boolean {
        val confText = persistedConf() ?: return false
        val cfg = try {
            Config.parse(confText.byteInputStream())
        } catch (e: Exception) {
            return false
        }
        repeat(retries) { attempt ->
            try {
                if (backend.getState(OrionTunnel) != Tunnel.State.UP) {
                    backend.setState(OrionTunnel, Tunnel.State.UP, cfg)
                }
                return true
            } catch (e: Exception) {
                if (attempt == retries - 1) return false
                try { Thread.sleep(4000) } catch (ie: InterruptedException) { return false }
            }
        }
        return false
    }

    fun persistedConf(): String? = try {
        openFileInput(CONF_FILE)?.bufferedReader()?.readText()
    } catch (e: Exception) {
        null
    }

    fun persistConf(text: String) {
        openFileOutput(CONF_FILE, MODE_PRIVATE).use { it.write(text.toByteArray()) }
    }

    companion object {
        private const val CONF_FILE = "orion.conf"
    }
}
