package dev.publiccode.app;

import android.app.Activity;
import android.content.Intent;
import android.content.SharedPreferences;
import android.net.Uri;
import android.os.Bundle;
import android.text.InputType;
import android.view.KeyEvent;
import android.view.Menu;
import android.view.MenuItem;
import android.webkit.CookieManager;
import android.webkit.WebResourceError;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.TextView;
import android.widget.Toast;

/**
 * Cliente Android de PublicCode: muestra la web de pcoded (sesiones, eventos y aprobaciones).
 *
 * <p>No decide nada (AGENTS.md, regla de arquitectura): toda la lógica vive en el demonio.
 */
public class MainActivity extends Activity {
    private static final String PREFS = "pcode";
    private static final String CLAVE_ORIGEN = "origen";
    private static final int MENU_RECARGAR = 1;
    private static final int MENU_CAMBIAR = 2;

    private WebView web;
    private String origen;

    @Override
    protected void onCreate(Bundle estado) {
        super.onCreate(estado);
        origen = prefs().getString(CLAVE_ORIGEN, null);
        if (origen == null) {
            mostrarFormulario();
        } else {
            mostrarWeb(origen + "/");
        }
    }

    private SharedPreferences prefs() {
        return getSharedPreferences(PREFS, MODE_PRIVATE);
    }

    private void mostrarFormulario() {
        web = null;
        int m = (int) (16 * getResources().getDisplayMetrics().density);
        LinearLayout raiz = new LinearLayout(this);
        raiz.setOrientation(LinearLayout.VERTICAL);
        raiz.setPadding(m, m, m, m);

        TextView titulo = new TextView(this);
        titulo.setText(R.string.conectar_titulo);
        titulo.setTextSize(22);
        raiz.addView(titulo);

        TextView ayuda = new TextView(this);
        ayuda.setText(R.string.conectar_ayuda);
        ayuda.setPadding(0, m / 2, 0, m);
        raiz.addView(ayuda);

        EditText url = new EditText(this);
        url.setHint(R.string.conectar_pista);
        url.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_URI);
        url.setSingleLine(true);
        raiz.addView(url);

        Button conectar = new Button(this);
        conectar.setText(R.string.conectar_boton);
        conectar.setOnClickListener(v -> {
            Conexion c = Conexion.parse(url.getText().toString());
            if (c == null) {
                url.setError(getString(R.string.error_url));
                return;
            }
            origen = c.origen();
            prefs().edit().putString(CLAVE_ORIGEN, origen).apply();
            mostrarWeb(c.urlInicial());
        });
        raiz.addView(conectar);
        setContentView(raiz);
        invalidateOptionsMenu();
    }

    private void mostrarWeb(String url) {
        web = new WebView(this);
        WebSettings s = web.getSettings();
        s.setJavaScriptEnabled(true); // la web del demonio usa JS (eventos SSE y JSON-RPC)
        s.setDomStorageEnabled(true);
        s.setAllowFileAccess(false);
        s.setAllowContentAccess(false);
        s.setSafeBrowsingEnabled(true);
        CookieManager.getInstance().setAcceptCookie(true);
        web.setWebViewClient(new Cliente());
        setContentView(web);
        invalidateOptionsMenu();
        web.loadUrl(url);
    }

    private void olvidarServidor() {
        prefs().edit().remove(CLAVE_ORIGEN).apply();
        CookieManager.getInstance().removeAllCookies(null);
        origen = null;
        mostrarFormulario();
    }

    @Override
    public boolean onCreateOptionsMenu(Menu menu) {
        menu.clear();
        if (web != null) {
            menu.add(Menu.NONE, MENU_RECARGAR, 0, R.string.recargar);
            menu.add(Menu.NONE, MENU_CAMBIAR, 1, R.string.cambiar_servidor);
        }
        return true;
    }

    @Override
    public boolean onPrepareOptionsMenu(Menu menu) {
        return onCreateOptionsMenu(menu);
    }

    @Override
    public boolean onOptionsItemSelected(MenuItem item) {
        if (item.getItemId() == MENU_RECARGAR && web != null) {
            web.reload();
            return true;
        }
        if (item.getItemId() == MENU_CAMBIAR) {
            olvidarServidor();
            return true;
        }
        return super.onOptionsItemSelected(item);
    }

    @Override
    public boolean onKeyDown(int codigo, KeyEvent evento) {
        if (codigo == KeyEvent.KEYCODE_BACK && web != null && web.canGoBack()) {
            web.goBack();
            return true;
        }
        return super.onKeyDown(codigo, evento);
    }

    /** Mantiene la navegación en el demonio y abre fuera cualquier otro enlace. */
    private final class Cliente extends WebViewClient {
        @Override
        public boolean shouldOverrideUrlLoading(WebView v, WebResourceRequest r) {
            String url = r.getUrl().toString();
            if (Conexion.mismoOrigen(origen, url)) {
                return false;
            }
            startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(url)));
            return true;
        }

        @Override
        public void onPageFinished(WebView v, String url) {
            CookieManager.getInstance().flush();
        }

        @Override
        public void onReceivedError(WebView v, WebResourceRequest r, WebResourceError e) {
            if (r.isForMainFrame()) {
                Toast.makeText(MainActivity.this, R.string.error_carga, Toast.LENGTH_LONG).show();
            }
        }

        @Override
        public void onReceivedHttpError(WebView v, WebResourceRequest r, WebResourceResponse e) {
            // 401 en la página principal: el demonio cambió de token; hay que volver a emparejar.
            if (r.isForMainFrame() && e.getStatusCode() == 401) {
                olvidarServidor();
            }
        }
    }
}
