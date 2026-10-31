package org.cuatrodetres.magi.plataforma

import android.content.Context
import org.cuatrodetres.magi.preferencias.PreferenciasMagi

/**
 * Implementación real de la fachada de plataforma. Una sola instancia por
 * proceso, creada por [org.cuatrodetres.magi.MagiApp] y accesible desde
 * `(application as MagiApp).plataforma`.
 *
 * La construcción no crea claves ni toca el Keystore: la clave del móvil se
 * crea al desbloquear el primer arranque (`firmante.asegurarClave()`) y la del
 * llavero, al guardar el primer secreto. [iniciar] solo restaura datos no
 * secretos (la carpeta SAF recordada).
 */
class ContenedorPlataforma(contexto: Context) : FachadaPlataforma {

    private val aplicacion: Context = contexto.applicationContext

    /** Ajustes no secretos en DataStore. */
    val preferencias: PreferenciasMagi = PreferenciasMagi.de(aplicacion)

    override val carpeta: CarpetaSaf = CarpetaSaf(aplicacion, preferencias)

    override val llavero: LlaveroKeystore = LlaveroKeystore(aplicacion)

    override val firmante: ClaveMovil = ClaveMovil(aplicacion)

    override val bloqueo: BloqueoBiometrico = BloqueoBiometrico(aplicacion)

    /** Guía de batería y su marca en DataStore. */
    val bateria: GuiaBateria = GuiaBateria(aplicacion, preferencias)

    /** Portapapeles del sistema: se vacía la clave de sincronización tras usarla. */
    val portapapeles: Portapapeles = Portapapeles(aplicacion)

    /** Restaura la carpeta recordada; lo llama el arranque de la app. */
    suspend fun iniciar() {
        carpeta.cargarCarpeta()
    }
}
