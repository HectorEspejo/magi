package org.cuatrodetres.magi.ui.vistaModelo

import androidx.lifecycle.ViewModel
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel

/**
 * Base de los ViewModels de la app.
 *
 * Usa un ámbito propio (no `viewModelScope`) para poder inyectar un despachador
 * en las pruebas de la JVM, donde no hay hilo principal de Android. En la app se
 * usa `Dispatchers.Main.immediate`, así que los flujos y las acciones de la
 * interfaz corren en el hilo principal.
 */
abstract class VistaModeloBase(ambito: CoroutineScope? = null) : ViewModel() {

    private val trabajo = SupervisorJob()

    protected val alcance: CoroutineScope = ambito ?: CoroutineScope(trabajo + Dispatchers.Main.immediate)

    override fun onCleared() {
        trabajo.cancel()
    }
}
