package org.cuatrodetres.magi.plataforma

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Reconocimiento de MagicOS/EMUI para la guía de batería (informe §4.6). */
class PruebaReglasBateria {

    @Test
    fun reconoceHonorYHuawei() {
        assertTrue(ReglasBateria.esMagicOs("HONOR"))
        assertTrue(ReglasBateria.esMagicOs("Honor"))
        assertTrue(ReglasBateria.esMagicOs("HUAWEI"))
        assertTrue(ReglasBateria.esMagicOs("  huawei  "))
    }

    @Test
    fun otrosFabricantesUsanPasosGenericos() {
        assertFalse(ReglasBateria.esMagicOs("samsung"))
        assertFalse(ReglasBateria.esMagicOs("Google"))
        assertFalse(ReglasBateria.esMagicOs("Xiaomi"))
        assertFalse(ReglasBateria.esMagicOs(null))
        assertFalse(ReglasBateria.esMagicOs("   "))
    }
}
