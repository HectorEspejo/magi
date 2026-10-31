package org.cuatrodetres.magi.ui.ajustes

import android.graphics.Bitmap
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.unit.IntSize
import com.google.zxing.BarcodeFormat
import com.google.zxing.qrcode.QRCodeWriter

/**
 * Código QR de la clave pública del móvil para compartirla con el escritorio.
 *
 * Se genera con ZXing core (sin ML Kit ni Play Services, D188) en una imagen de
 * mapa de bits: se pinta de una vez y no se recalcula en cada fotograma.
 */
@Composable
fun LienzoQr(texto: String, modifier: Modifier = Modifier) {
    val imagen: ImageBitmap? = remember(texto) {
        runCatching {
            val matriz = QRCodeWriter().encode(texto, BarcodeFormat.QR_CODE, 512, 512)
            val mapa = Bitmap.createBitmap(matriz.width, matriz.height, Bitmap.Config.ARGB_8888)
            for (y in 0 until matriz.height) {
                for (x in 0 until matriz.width) {
                    mapa.setPixel(
                        x,
                        y,
                        if (matriz.get(x, y)) android.graphics.Color.BLACK else android.graphics.Color.WHITE,
                    )
                }
            }
            mapa.asImageBitmap()
        }.getOrNull()
    }
    if (imagen == null) {
        Text(
            text = "No se pudo generar el QR.",
            style = MaterialTheme.typography.bodyMedium,
            color = Color.Red,
        )
        return
    }
    Canvas(
        modifier = modifier
            .fillMaxWidth()
            .aspectRatio(1f)
            .background(Color.White),
    ) {
        drawImage(
            image = imagen,
            dstSize = IntSize(size.width.toInt(), size.height.toInt()),
        )
    }
}
