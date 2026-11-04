package org.cuatrodetres.magi.plataforma

/** Reglas de la guía de batería (informe §4.6), sin Android para probarlas en la JVM. */
object ReglasBateria {

    /**
     * Fabricantes con MagicOS/EMUI y pasos propios. Honor (MagicOS) y Huawei
     * (EMUI) son los que el informe menciona; el resto usa los pasos genéricos.
     */
    fun esMagicOs(fabricante: String?): Boolean {
        val nombre = fabricante?.trim()?.lowercase() ?: return false
        return nombre.contains("honor") || nombre.contains("huawei")
    }
}
