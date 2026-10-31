package org.cuatrodetres.magi

import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Prueba de humo del emulador (API 30, x86_64): la app arranca.
 *
 * TODO(F11): ampliarla cuando exista la puerta UniFFI para cargar el núcleo
 * nativo, importar un fichero de ejemplo y pintar sus hosts (checklist).
 */
@RunWith(AndroidJUnit4::class)
class PruebaHumo {

    @Test
    fun laActividadPrincipalArranca() {
        ActivityScenario.launch(MainActivity::class.java).use { escenario ->
            escenario.onActivity { actividad ->
                assertEquals("org.cuatrodetres.magi", actividad.packageName)
            }
        }
    }
}
