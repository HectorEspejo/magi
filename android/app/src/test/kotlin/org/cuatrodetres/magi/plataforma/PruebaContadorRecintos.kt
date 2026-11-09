package org.cuatrodetres.magi.plataforma

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Regresión de la revisión adversarial: con recintos seguros anidados,
 * FLAG_SECURE solo se retira al salir el último (si no, un diálogo de
 * contraseña sobre una pantalla con secretos dejaba la ventana capturable).
 */
class PruebaContadorRecintos {

    @Test
    fun elFlagSoloSeAplicaAlEntrarElPrimeroYSeRetiraAlSalirElUltimo() {
        val contador = ContadorRecintos<String>()
        assertTrue(contador.entrar("actividad"))
        assertFalse(contador.entrar("actividad"))
        assertEquals(2, contador.cuantos("actividad"))

        assertFalse(contador.salir("actividad"))
        assertEquals(1, contador.cuantos("actividad"))
        assertTrue(contador.salir("actividad"))
        assertEquals(0, contador.cuantos("actividad"))
    }

    @Test
    fun cadaActividadSeCuentaAparte() {
        val contador = ContadorRecintos<String>()
        assertTrue(contador.entrar("a"))
        assertTrue(contador.entrar("b"))
        // Cada anfitrión se cuenta aparte: salir de "a" es su último recinto.
        assertTrue(contador.salir("a"))
        assertEquals(0, contador.cuantos("a"))
        assertEquals(1, contador.cuantos("b"))
        assertTrue(contador.salir("b"))
    }
}
