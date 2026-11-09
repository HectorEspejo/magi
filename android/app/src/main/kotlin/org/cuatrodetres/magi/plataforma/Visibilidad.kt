package org.cuatrodetres.magi.plataforma

import android.os.Handler
import android.os.Looper
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Visibilidad de la app (informe §2 y §8, D266 y D279).
 *
 * Desde la Fase 14, el sondeo de Flota, los refrescos de Servicios y los logs
 * solo corren con su pantalla a la vista: en segundo plano no se gasta batería
 * ni se mantienen procesos en el host. Las acciones y las ejecuciones sí
 * siguen, y el servicio en primer plano las cuenta.
 *
 * Se apoya en ProcessLifecycleOwner, que combina el ciclo de vida de todas las
 * Activity: ON_START significa que hay alguna a la vista y ON_STOP que ninguna
 * (la app está en segundo plano, aunque el proceso siga vivo por el servicio).
 * Las pantallas pueden usar además repeatOnLifecycle(RESUMED), que es lo que
 * corta un temporizador al taparla con otra Activity.
 *
 * LifecycleRegistry exige registrar y quitar observadores en el hilo principal:
 * si se construye desde otro hilo (pruebas instrumentadas, por ejemplo), el
 * registro se difiere al hilo principal y el resto de la clase no cambia.
 */
class Visibilidad(
    private val propietario: LifecycleOwner = ProcessLifecycleOwner.get(),
) {

    private val _aLaVista = MutableStateFlow(
        propietario.lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED),
    )

    /** true = hay alguna pantalla de MAGI a la vista; false = app en segundo plano. */
    val aLaVista: StateFlow<Boolean> = _aLaVista.asStateFlow()

    private val observador = LifecycleEventObserver { _, evento ->
        when (evento) {
            Lifecycle.Event.ON_START -> _aLaVista.value = true
            Lifecycle.Event.ON_STOP -> _aLaVista.value = false
            else -> Unit
        }
    }

    init {
        enHiloPrincipal {
            propietario.lifecycle.addObserver(observador)
        }
    }

    /** Consulta puntual, sin observar el flujo. */
    fun estaALaVista(): Boolean = _aLaVista.value

    /** Deja de observar el ciclo de vida (al cerrar la app o al liberar el contenedor). */
    fun soltar() {
        enHiloPrincipal {
            propietario.lifecycle.removeObserver(observador)
        }
    }

    companion object {
        /** Consulta directa al ciclo de vida del proceso (sin flujo ni observador). */
        fun estaLaAppALaVista(): Boolean =
            ProcessLifecycleOwner.get().lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)

        /** Ejecuta [bloque] en el hilo principal, ya sea ahora o en el próximo turno. */
        private fun enHiloPrincipal(bloque: () -> Unit) {
            if (Looper.myLooper() == Looper.getMainLooper()) {
                bloque()
            } else {
                Handler(Looper.getMainLooper()).post { bloque() }
            }
        }
    }
}
