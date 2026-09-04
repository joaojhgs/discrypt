package chat.discrypt.desktop

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import org.json.JSONObject

class MainActivity : TauriActivity() {
  private var voiceWebView: WebView? = null
  private var pendingBluetoothRequestId: String? = null
  private val bluetoothPermissionLauncher =
    registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
      pendingBluetoothRequestId?.let { dispatchBluetoothPermissionResult(it, granted) }
      pendingBluetoothRequestId = null
    }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    voiceWebView = webView
    webView.addJavascriptInterface(AndroidVoicePermissions(), "DiscryptAndroidVoice")
  }

  private fun dispatchBluetoothPermissionResult(requestId: String, granted: Boolean) {
    val detail =
      "{requestId:${JSONObject.quote(requestId)},granted:$granted}"
    voiceWebView?.post {
      voiceWebView?.evaluateJavascript(
        "window.dispatchEvent(new CustomEvent('discrypt:android-bluetooth-audio-permission',{detail:$detail}))",
        null,
      )
    }
  }

  private inner class AndroidVoicePermissions {
    @JavascriptInterface
    fun requestBluetoothAudioAccess(requestId: String) {
      if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) {
        dispatchBluetoothPermissionResult(requestId, true)
        return
      }
      if (
        ContextCompat.checkSelfPermission(
          this@MainActivity,
          Manifest.permission.BLUETOOTH_CONNECT,
        ) == PackageManager.PERMISSION_GRANTED
      ) {
        dispatchBluetoothPermissionResult(requestId, true)
        return
      }
      pendingBluetoothRequestId = requestId
      runOnUiThread {
        bluetoothPermissionLauncher.launch(Manifest.permission.BLUETOOTH_CONNECT)
      }
    }
  }
}
