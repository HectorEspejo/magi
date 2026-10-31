package org.cuatrodetres.magi.ui.dialogos

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.MultiFormatReader
import com.google.zxing.NotFoundException
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.common.HybridBinarizer
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.ui.componentes.BotonCabina
import org.cuatrodetres.magi.ui.tema.Paleta
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Lector de QR con CameraX + ZXing core (sin ML Kit ni Play Services, D188).
 *
 * La cámara solo se usa para leer el QR de la clave de sincronización; no
 * guarda imágenes. Al primer QR válido se llama a [alLeido] una sola vez.
 */
@Composable
fun EscanerQr(
    alLeido: (String) -> Unit,
    alCerrar: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val contexto = LocalContext.current
    val propietarioCiclo = LocalLifecycleOwner.current
    var conPermiso by remember {
        mutableStateOf(
            ContextCompat.checkSelfPermission(contexto, Manifest.permission.CAMERA) ==
                PackageManager.PERMISSION_GRANTED,
        )
    }
    val lanzadorPermiso = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { concedido ->
        conPermiso = concedido
    }
    LaunchedEffect(Unit) {
        if (!conPermiso) lanzadorPermiso.launch(Manifest.permission.CAMERA)
    }
    if (!conPermiso) {
        Column(
            modifier = modifier.fillMaxSize().background(Paleta.FONDO).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(
                text = stringResource(R.string.qr_sin_permiso),
                style = MaterialTheme.typography.bodyMedium,
                color = Paleta.TEXTO,
                modifier = Modifier.fillMaxWidth(),
            )
            BotonCabina(
                texto = stringResource(R.string.qr_volver),
                alPulsar = alCerrar,
                primario = true,
            )
        }
        return
    }

    val ejecutor = remember { Executors.newSingleThreadExecutor() }
    val leido = remember { AtomicBoolean(false) }
    DisposableEffect(Unit) {
        onDispose {
            ejecutor.shutdown()
            val futuro = ProcessCameraProvider.getInstance(contexto)
            futuro.addListener(
                { runCatching { futuro.get().unbindAll() } },
                ContextCompat.getMainExecutor(contexto),
            )
        }
    }
    Box(modifier = modifier.fillMaxSize().background(Paleta.FONDO)) {
        AndroidView(
            modifier = Modifier.fillMaxSize(),
            factory = { ctx ->
                val vista = PreviewView(ctx).apply {
                    scaleType = PreviewView.ScaleType.FILL_CENTER
                }
                val futuro = ProcessCameraProvider.getInstance(ctx)
                futuro.addListener({
                    runCatching {
                        val proveedor = futuro.get()
                        val vistaPrevia = Preview.Builder().build().also {
                            it.setSurfaceProvider(vista.surfaceProvider)
                        }
                        val analisis = ImageAnalysis.Builder()
                            .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                            .build()
                        analisis.setAnalyzer(ejecutor) { imagen ->
                            try {
                                if (!leido.get()) {
                                    val texto = decodificarQr(imagen)
                                    if (texto != null && leido.compareAndSet(false, true)) {
                                        vista.post { alLeido(texto) }
                                    }
                                }
                            } finally {
                                imagen.close()
                            }
                        }
                        proveedor.unbindAll()
                        proveedor.bindToLifecycle(
                            propietarioCiclo,
                            CameraSelector.DEFAULT_BACK_CAMERA,
                            vistaPrevia,
                            analisis,
                        )
                    }
                }, ContextCompat.getMainExecutor(ctx))
                vista
            },
        )
        Column(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .background(Paleta.SUPERFICIE)
                .padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                text = stringResource(R.string.qr_instruccion),
                style = MaterialTheme.typography.bodyMedium,
                color = Paleta.TEXTO,
            )
            BotonCabina(
                texto = stringResource(R.string.qr_volver),
                alPulsar = alCerrar,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

/**
 * Decodifica el QR del plano de luminancia de la imagen de la cámara.
 * La imagen llega en YUV_420_888; basta el plano Y para ZXing.
 */
private fun decodificarQr(imagen: ImageProxy): String? {
    val plano = imagen.planes.firstOrNull() ?: return null
    val ancho = imagen.width
    val alto = imagen.height
    if (ancho <= 0 || alto <= 0) return null
    val buffer = plano.buffer
    val saltorFila = plano.rowStride
    val pasoPixel = plano.pixelStride
    val datos = ByteArray(ancho * alto)
    if (pasoPixel == 1 && saltorFila == ancho) {
        buffer.rewind()
        buffer.get(datos)
    } else {
        val fila = ByteArray(saltorFila)
        for (y in 0 until alto) {
            buffer.position(y * saltorFila)
            val disponibles = minOf(saltorFila, buffer.remaining())
            buffer.get(fila, 0, disponibles)
            for (x in 0 until ancho) {
                datos[y * ancho + x] = fila[x * pasoPixel]
            }
        }
    }
    return try {
        val fuente = PlanarYUVLuminanceSource(datos, ancho, alto, 0, 0, ancho, alto, false)
        val mapa = BinaryBitmap(HybridBinarizer(fuente))
        val lector = MultiFormatReader().apply {
            setHints(
                mapOf(
                    DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE),
                    DecodeHintType.TRY_HARDER to true,
                ),
            )
        }
        lector.decode(mapa).text
    } catch (e: NotFoundException) {
        null
    } catch (e: Exception) {
        null
    }
}
