package dev.publiccode.app;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;

import org.junit.Test;

public class ConexionTest {
    @Test
    public void parseUrlDelDemonio() {
        Conexion c = Conexion.parse("  http://192.168.1.10:7878/?t=abc123  ");
        assertEquals("http://192.168.1.10:7878", c.origen());
        assertEquals("abc123", c.token());
        assertEquals("http://192.168.1.10:7878/?t=abc123", c.urlInicial());
    }

    @Test
    public void parseHttpsSinPuertoYConOtrosParametros() {
        Conexion c = Conexion.parse("HTTPS://Pcode.Example.org/?x=1&t=zz");
        assertEquals("https://pcode.example.org", c.origen());
        assertEquals("zz", c.token());
    }

    @Test
    public void parseIpv6() {
        assertEquals("http://[::1]:7878", Conexion.parse("http://[::1]:7878/?t=a").origen());
    }

    @Test
    public void rechazaUrlsInvalidas() {
        assertNull(Conexion.parse(null));
        assertNull(Conexion.parse(""));
        assertNull(Conexion.parse("ftp://h/?t=a"));
        assertNull(Conexion.parse("javascript:alert(1)"));
        assertNull(Conexion.parse("http://h:7878/"));
        assertNull(Conexion.parse("http://h:7878/?t="));
        assertNull(Conexion.parse("http:///?t=a"));
        assertNull(Conexion.parse("http://h/ ?t=a b"));
    }

    @Test
    public void mismoOrigen() {
        String o = "http://192.168.1.10:7878";
        assertTrue(Conexion.mismoOrigen(o, "http://192.168.1.10:7878/rpc"));
        assertFalse(Conexion.mismoOrigen(o, "http://192.168.1.10:7879/"));
        assertFalse(Conexion.mismoOrigen(o, "https://192.168.1.10:7878/"));
        assertFalse(Conexion.mismoOrigen(o, "http://evil.example/"));
        assertTrue(Conexion.mismoOrigen("https://a.org", "https://a.org:443/x"));
        assertFalse(Conexion.mismoOrigen(null, "http://a/"));
    }
}
