package dev.publiccode.app;

import java.net.URI;
import java.net.URISyntaxException;
import java.util.Locale;

/**
 * Datos para conectar con pcoded a partir de la URL que imprime {@code pcode daemon}.
 *
 * <p>Lógica pura (sin Android) para poder probarla en la JVM. La app solo guarda el origen: el
 * token viaja una vez en {@code ?t=} y el demonio lo cambia por una cookie HttpOnly.
 */
public final class Conexion {
    private final String origen;
    private final String token;

    private Conexion(String origen, String token) {
        this.origen = origen;
        this.token = token;
    }

    /** Interpreta la URL pegada por el usuario; devuelve {@code null} si no es válida. */
    public static Conexion parse(String texto) {
        if (texto == null) {
            return null;
        }
        URI uri;
        try {
            uri = new URI(texto.trim());
        } catch (URISyntaxException e) {
            return null;
        }
        String esquema = uri.getScheme() == null ? "" : uri.getScheme().toLowerCase(Locale.ROOT);
        if (!esquema.equals("http") && !esquema.equals("https")) {
            return null;
        }
        String host = uri.getHost();
        if (host == null || host.isEmpty()) {
            return null;
        }
        String token = parametro(uri.getRawQuery(), "t");
        if (token == null || token.isEmpty()) {
            return null;
        }
        String puerto = uri.getPort() == -1 ? "" : ":" + uri.getPort();
        String h = host.contains(":") && !host.startsWith("[") ? "[" + host + "]" : host;
        return new Conexion(esquema + "://" + h.toLowerCase(Locale.ROOT) + puerto, token);
    }

    /** Origen del demonio, p. ej. {@code http://192.168.1.10:7878}. */
    public String origen() {
        return origen;
    }

    /** Token de emparejamiento. */
    public String token() {
        return token;
    }

    /** URL de la primera carga: entrega el token para que el demonio fije la cookie. */
    public String urlInicial() {
        return origen + "/?t=" + token;
    }

    /** Indica si {@code url} pertenece al origen {@code origen} (el resto se abre fuera). */
    public static boolean mismoOrigen(String origen, String url) {
        if (origen == null || url == null) {
            return false;
        }
        try {
            URI a = new URI(origen);
            URI b = new URI(url);
            return a.getScheme() != null
                    && a.getScheme().equalsIgnoreCase(b.getScheme())
                    && a.getHost() != null
                    && a.getHost().equalsIgnoreCase(b.getHost())
                    && puerto(a) == puerto(b);
        } catch (URISyntaxException e) {
            return false;
        }
    }

    private static int puerto(URI u) {
        if (u.getPort() != -1) {
            return u.getPort();
        }
        return "https".equalsIgnoreCase(u.getScheme()) ? 443 : 80;
    }

    private static String parametro(String query, String nombre) {
        if (query == null) {
            return null;
        }
        for (String par : query.split("&")) {
            int i = par.indexOf('=');
            if (i > 0 && par.substring(0, i).equals(nombre)) {
                return par.substring(i + 1);
            }
        }
        return null;
    }
}
