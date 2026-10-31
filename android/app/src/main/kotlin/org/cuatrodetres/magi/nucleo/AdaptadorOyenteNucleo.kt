package org.cuatrodetres.magi.nucleo

import org.cuatrodetres.magi.ui.modelos.EstadoSincronizacionVista
import org.cuatrodetres.magi.ui.modelos.PantallaVista
import org.cuatrodetres.magi.ui.modelos.PeticionDialogoVista
import org.cuatrodetres.magi.ui.modelos.SesionVista
import org.cuatrodetres.magi.ui.modelos.TunelVista
import org.cuatrodetres.magi.ui.modelos.numerarSesiones

/**
 * Oyente del núcleo (interfaz foreign generada por UniFFI) que reenvía cada
 * notificación a la fachada de la interfaz y a quien corresponda:
 *
 * - pantallas por tramos, listas completas de sesiones y túneles, diálogos y
 *   estado de la sincronización → flujos de `FuenteNucleoReal`;
 * - al cambiar las listas, el número de sesiones y de túneles activos → el
 *   servicio en primer plano (`ServicioSesiones.actualizar`).
 *
 * Los métodos se llaman desde hilos del núcleo Rust: los flujos son seguros
 * entre hilos y los contadores se guardan como volátiles.
 */
class AdaptadorOyenteNucleo(
    private val alRecibirSesiones: (List<SesionVista>) -> Unit,
    private val alRecibirTuneles: (List<TunelVista>) -> Unit,
    private val alRecibirPantalla: (PantallaVista) -> Unit,
    private val alRecibirPeticion: (PeticionDialogoVista?) -> Unit,
    private val alRecibirSincronizacion: (EstadoSincronizacionVista) -> Unit,
    private val alCambiarTrabajoVivo: (sesiones: Int, tuneles: Int) -> Unit,
) : OyenteNucleo {

    @Volatile
    private var numeroSesiones: Int = 0

    @Volatile
    private var numeroTunelesActivos: Int = 0

    override fun pantalla(pantalla: Pantalla) {
        alRecibirPantalla(pantalla.aVista())
    }

    override fun sesiones(lista: List<Sesion>) {
        val sesiones = numerarSesiones(lista.map { it.aVista() })
        numeroSesiones = sesiones.size
        alRecibirSesiones(sesiones)
        alCambiarTrabajoVivo(numeroSesiones, numeroTunelesActivos)
    }

    override fun tuneles(lista: List<TunelVivo>) {
        val tuneles = lista.map { it.aVista() }
        numeroTunelesActivos = tuneles.tunelesActivos()
        alRecibirTuneles(tuneles)
        alCambiarTrabajoVivo(numeroSesiones, numeroTunelesActivos)
    }

    override fun dialogo(peticion: PeticionDialogo) {
        alRecibirPeticion(peticion.aVista())
    }

    override fun sincronizacion(estado: EstadoSincronizacion) {
        alRecibirSincronizacion(estado.aVista())
    }
}
