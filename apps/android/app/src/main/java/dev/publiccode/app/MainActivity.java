package dev.publiccode.app;

import android.app.Activity;
import android.content.Intent;
import android.content.SharedPreferences;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.net.Uri;
import android.os.Bundle;
import android.text.InputType;
import android.view.Gravity;
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
import android.widget.ScrollView;
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

    private int dp(float v) {
        return (int) (v * getResources().getDisplayMetrics().density);
    }

    private int color(int id) {
        return getColor(id);
    }

    /** Fondo redondeado (tarjetas, campos y botones), con borde opcional. */
    private GradientDrawable redondo(int fondo, float radio, int borde) {
        GradientDrawable g = new GradientDrawable();
        g.setColor(fondo);
        g.setCornerRadius(dp(radio));
        if (borde != 0) {
            g.setStroke(dp(1.5f), borde);
        }
        return g;
    }

    private TextView texto(int id, float sp, int colorId, boolean negrita) {
        TextView t = new TextView(this);
        t.setText(id);
        t.setTextSize(sp);
        t.setTextColor(color(colorId));
        if (negrita) {
            t.setTypeface(Typeface.create("sans-serif-medium", Typeface.NORMAL));
        }
        return t;
    }

    private void mostrarFormulario() {
        web = null;
        int m = dp(20);
        LinearLayout raiz = new LinearLayout(this);
        raiz.setOrientation(LinearLayout.VERTICAL);
        raiz.setPadding(m, dp(28), m, m);

        // Cabecera: logo «>_» y título.
        TextView logo = new TextView(this);
        logo.setText(">_");
        logo.setTextSize(26);
        logo.setTypeface(Typeface.MONOSPACE, Typeface.BOLD);
        logo.setTextColor(color(R.color.primario));
        logo.setGravity(Gravity.CENTER);
        logo.setBackground(redondo(color(R.color.contenedor), 24, 0));
        raiz.addView(logo, new LinearLayout.LayoutParams(dp(72), dp(72)));

        TextView titulo = texto(R.string.conectar_titulo, 26, R.color.texto, true);
        titulo.setPadding(0, dp(20), 0, dp(4));
        raiz.addView(titulo);
        TextView sub = texto(R.string.subtitulo, 15, R.color.texto_2, false);
        sub.setPadding(0, 0, 0, dp(20));
        raiz.addView(sub);

        // Tarjeta con el campo y el botón.
        LinearLayout tarjeta = new LinearLayout(this);
        tarjeta.setOrientation(LinearLayout.VERTICAL);
        tarjeta.setPadding(m, m, m, m);
        tarjeta.setBackground(redondo(color(R.color.superficie), 24, 0));
        tarjeta.setElevation(dp(3));

        TextView etiqueta = texto(R.string.url_etiqueta, 13, R.color.texto_2, true);
        etiqueta.setPadding(dp(4), 0, 0, dp(6));
        tarjeta.addView(etiqueta);

        EditText url = new EditText(this);
        url.setHint(R.string.conectar_pista);
        url.setHintTextColor(color(R.color.texto_2));
        url.setTextColor(color(R.color.texto));
        url.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_URI);
        url.setSingleLine(true);
        url.setPadding(dp(14), dp(14), dp(14), dp(14));
        url.setBackground(redondo(color(R.color.superficie_2), 14, color(R.color.linea)));
        url.setOnFocusChangeListener((v, focus) -> v.setBackground(redondo(
                color(R.color.superficie_2), 14,
                color(focus ? R.color.primario : R.color.linea))));
        tarjeta.addView(url);

        Button conectar = new Button(this);
        conectar.setText(R.string.conectar_boton);
        conectar.setAllCaps(false);
        conectar.setTextSize(16);
        conectar.setTextColor(color(R.color.sobre_primario));
        conectar.setBackground(redondo(color(R.color.primario), 999, 0));
        conectar.setStateListAnimator(null);
        conectar.setElevation(dp(2));
        LinearLayout.LayoutParams lb = new LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT, dp(52));
        lb.topMargin = dp(16);
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
        tarjeta.addView(conectar, lb);
        raiz.addView(tarjeta);

        // Tarjeta tonal con los pasos.
        LinearLayout pasos = new LinearLayout(this);
        pasos.setOrientation(LinearLayout.VERTICAL);
        pasos.setPadding(m, m, m, m);
        pasos.setBackground(redondo(color(R.color.contenedor), 24, 0));
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT);
        lp.topMargin = dp(16);
        TextView pt = texto(R.string.pasos_titulo, 16, R.color.sobre_contenedor, true);
        pt.setPadding(0, 0, 0, dp(8));
        pasos.addView(pt);
        for (int id : new int[] {R.string.paso1, R.string.paso2, R.string.paso3}) {
            TextView p = texto(id, 14, R.color.sobre_contenedor, false);
            p.setPadding(0, dp(4), 0, dp(4));
            if (id == R.string.paso2) {
                p.setTypeface(Typeface.MONOSPACE);
            }
            pasos.addView(p);
        }
        raiz.addView(pasos, lp);

        ScrollView scroll = new ScrollView(this);
        scroll.setBackgroundColor(color(R.color.fondo));
        scroll.addView(raiz);
        setContentView(scroll);
        invalidateOptionsMenu();
    }

    private void mostrarWeb(String url) {
        web = new WebView(this);
        web.setBackgroundColor(color(R.color.fondo));
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
