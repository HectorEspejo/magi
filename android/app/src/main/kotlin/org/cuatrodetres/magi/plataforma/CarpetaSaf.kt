package org.cuatrodetres.magi.plataforma

import android.content.ContentResolver
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import org.cuatrodetres.magi.R
import org.cuatrodetres.magi.preferencias.PreferenciasMagi
import java.io.ByteArrayOutputStream
import java.io.IOException

/**
 * Carpeta sincronizada por Storage Access Framework, SOLO lectura (informe
 * §4.8 y §5.2). El móvil nunca escribe en la carpeta: el permiso persistente se
 * toma únicamente de lectura y el trait no expone escritura.
 *
 * Se listan y leen solo los ficheros `magi-<uuid>.age` canónicos y de tamaño
 * válido (T81): copias en conflicto, temporales, subcarpetas y nombres no
 * canónicos se ignoran. Aun así, la pasada del núcleo vuelve a validar nombre y
 * tamaño: SAF es entrada no fiable.
 *
 * La URI elegida se recuerda en DataStore (no es un secreto) y [cargarCarpeta]
 * la restaura al arrancar, de modo que tras reiniciar el móvil no hay que
 * volver a elegirla.
 */
class CarpetaSaf(
    private val contexto: Context,
    private val preferencias: PreferenciasMagi,
) : CarpetaMovil {

    @Volatile
    private var carpetaGuardada: Uri? = null

    /** Restaura la carpeta recordada; lo llama el arranque de la app. */
    suspend fun cargarCarpeta(): Uri? {
        val guardada = preferencias.primeraLectura().carpetaUri?.let(Uri::parse)
        carpetaGuardada = guardada
        return guardada
    }

    /** URI recordada, para enseñarla en Ajustes. */
    fun uriActual(): Uri? = carpetaGuardada

    /**
     * Intent del selector del sistema. Usa StartActivityForResult (no
     * OpenDocumentTree) porque hacen falta los flags del resultado para tomar
     * el permiso persistente.
     */
    fun crearIntentElegirCarpeta(): Intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
        addFlags(
            Intent.FLAG_GRANT_READ_URI_PERMISSION or
                Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
        )
        carpetaGuardada?.let { putExtra(DocumentsContract.EXTRA_INITIAL_URI, it) }
    }

    /**
     * Toma el permiso persistente SOLO de lectura y recuerda la carpeta. Falla
     * si el proveedor no concede permiso persistente (p. ej. una carpeta
     * temporal).
     */
    suspend fun recordarCarpeta(uri: Uri) {
        if (uri.scheme != ContentResolver.SCHEME_CONTENT) {
            throw ErrorPlataforma.Carpeta(
                contexto.getString(R.string.plataforma_carpeta_no_valida)
            )
        }
        try {
            contexto.contentResolver.takePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION,
            )
        } catch (e: SecurityException) {
            throw ErrorPlataforma.Carpeta(
                contexto.getString(R.string.plataforma_carpeta_sin_permiso_persistente),
                e,
            )
        }
        val vigente = contexto.contentResolver.persistedUriPermissions.any {
            it.uri == uri && it.isReadPermission
        }
        if (!vigente) {
            throw ErrorPlataforma.Carpeta(
                contexto.getString(R.string.plataforma_carpeta_sin_permiso_persistente)
            )
        }
        preferencias.guardarCarpetaUri(uri.toString())
        carpetaGuardada = uri
    }

    /** Suelta el permiso y olvida la carpeta (para dejar de sincronizar). */
    suspend fun olvidarCarpeta() {
        val actual = carpetaGuardada
        if (actual != null) {
            try {
                contexto.contentResolver.releasePersistableUriPermission(
                    actual,
                    Intent.FLAG_GRANT_READ_URI_PERMISSION,
                )
            } catch (_: SecurityException) {
                // El permiso ya no estaba: nada que soltar.
            }
        }
        preferencias.guardarCarpetaUri(null)
        carpetaGuardada = null
    }

    /** Si la carpeta recordada tiene al menos un fichero de equipo de MAGI. */
    fun hayFicherosMagi(): Boolean = listar().isNotEmpty()

    override fun listar(): List<FicheroCarpeta> {
        val raiz = requireCarpeta()
        val idArbol = idDelArbol(raiz)
        val hijosUri = DocumentsContract.buildChildDocumentsUriUsingTree(raiz, idArbol)
        val ficheros = consultarHijos(hijosUri)
            .filter { hijo -> hijo.mime != DocumentsContract.Document.MIME_TYPE_DIR }
            .map { hijo ->
                FicheroCarpeta(
                    nombre = hijo.nombre,
                    // Un tamaño desconocido no se puede validar contra el tope: se descarta.
                    tamano = hijo.tamano ?: -1L,
                    modificadoEn = hijo.modificadoMs?.takeIf { it > 0L }?.div(1_000L) ?: 0L,
                )
            }
        return ReglasCarpeta.filtrar(ficheros)
    }

    override fun leer(nombre: String, maximo: Long): ByteArray {
        if (!ReglasCarpeta.nombreValido(nombre)) {
            throw ErrorPlataforma.Carpeta(
                contexto.getString(R.string.plataforma_carpeta_nombre_invalido, nombre)
            )
        }
        if (maximo <= 0L) {
            throw ErrorPlataforma.Carpeta(
                contexto.getString(R.string.plataforma_carpeta_tamano_no_valido)
            )
        }
        val tope = minOf(maximo, ReglasCarpeta.TAMANO_MAXIMO)
        val raiz = requireCarpeta()
        val hijo = buscarHijo(raiz, nombre) ?: throw ErrorPlataforma.Carpeta(
            contexto.getString(R.string.plataforma_carpeta_fichero_no_encontrado, nombre)
        )
        val uriDocumento = DocumentsContract.buildDocumentUriUsingTree(raiz, hijo.id)
        val tamano = hijo.tamano ?: tamanoDe(uriDocumento)
        if (tamano != null && tamano > tope) {
            throw ErrorPlataforma.Carpeta(
                contexto.getString(R.string.plataforma_carpeta_fichero_grande, nombre, tope)
            )
        }
        return leerBytes(uriDocumento, tope, nombre)
    }

    // -----------------------------------------------------------------------
    // Interno
    // -----------------------------------------------------------------------

    private data class Hijo(
        val id: String,
        val nombre: String,
        val mime: String?,
        val tamano: Long?,
        val modificadoMs: Long?,
    )

    private fun requireCarpeta(): Uri {
        val uri = carpetaGuardada ?: throw ErrorPlataforma.Carpeta(
            contexto.getString(R.string.plataforma_carpeta_sin_elegir)
        )
        val vigente = try {
            contexto.contentResolver.persistedUriPermissions.any {
                it.uri == uri && it.isReadPermission
            }
        } catch (_: SecurityException) {
            false
        }
        if (!vigente) {
            throw ErrorPlataforma.Carpeta(
                contexto.getString(R.string.plataforma_carpeta_permiso_retirado)
            )
        }
        return uri
    }

    private fun idDelArbol(raiz: Uri): String = try {
        DocumentsContract.getTreeDocumentId(raiz)
    } catch (e: IllegalArgumentException) {
        throw ErrorPlataforma.Carpeta(
            contexto.getString(R.string.plataforma_carpeta_no_es_arbol),
            e,
        )
    }

    private fun buscarHijo(raiz: Uri, nombre: String): Hijo? {
        val idArbol = idDelArbol(raiz)
        val hijosUri = DocumentsContract.buildChildDocumentsUriUsingTree(raiz, idArbol)
        return consultarHijos(hijosUri)
            .firstOrNull { it.nombre == nombre && it.mime != DocumentsContract.Document.MIME_TYPE_DIR }
    }

    private fun consultarHijos(uri: Uri): List<Hijo> {
        val proyeccion = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_SIZE,
            DocumentsContract.Document.COLUMN_LAST_MODIFIED,
        )
        val hijos = mutableListOf<Hijo>()
        try {
            val cursor = contexto.contentResolver.query(uri, proyeccion, null, null, null)
                ?: throw ErrorPlataforma.Carpeta(
                    contexto.getString(R.string.plataforma_carpeta_ilegible)
                )
            cursor.use {
                val columnaId = it.getColumnIndex(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
                val columnaNombre = it.getColumnIndex(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
                val columnaMime = it.getColumnIndex(DocumentsContract.Document.COLUMN_MIME_TYPE)
                val columnaTamano = it.getColumnIndex(DocumentsContract.Document.COLUMN_SIZE)
                val columnaFecha = it.getColumnIndex(DocumentsContract.Document.COLUMN_LAST_MODIFIED)
                if (columnaId < 0 || columnaNombre < 0) {
                    throw ErrorPlataforma.Carpeta(
                        contexto.getString(R.string.plataforma_carpeta_ilegible)
                    )
                }
                while (it.moveToNext()) {
                    val id = it.getString(columnaId) ?: continue
                    val nombre = it.getString(columnaNombre) ?: continue
                    val mime = if (columnaMime >= 0 && !it.isNull(columnaMime)) {
                        it.getString(columnaMime)
                    } else {
                        null
                    }
                    val tamano = if (columnaTamano >= 0 && !it.isNull(columnaTamano)) {
                        it.getLong(columnaTamano)
                    } else {
                        null
                    }
                    val fecha = if (columnaFecha >= 0 && !it.isNull(columnaFecha)) {
                        it.getLong(columnaFecha)
                    } else {
                        null
                    }
                    hijos += Hijo(id, nombre, mime, tamano, fecha)
                }
            }
        } catch (e: ErrorPlataforma.Carpeta) {
            throw e
        } catch (e: SecurityException) {
            throw ErrorPlataforma.Carpeta(
                contexto.getString(R.string.plataforma_carpeta_permiso_retirado),
                e,
            )
        } catch (e: IllegalArgumentException) {
            throw ErrorPlataforma.Carpeta(
                contexto.getString(R.string.plataforma_carpeta_ilegible),
                e,
            )
        } catch (e: RuntimeException) {
            throw ErrorPlataforma.Carpeta(
                contexto.getString(R.string.plataforma_carpeta_ilegible),
                e,
            )
        }
        return hijos
    }

    private fun tamanoDe(uriDocumento: Uri): Long? {
        val proyeccion = arrayOf(DocumentsContract.Document.COLUMN_SIZE)
        return try {
            contexto.contentResolver.query(uriDocumento, proyeccion, null, null, null)?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val columna = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_SIZE)
                    if (columna >= 0 && !cursor.isNull(columna)) cursor.getLong(columna) else null
                } else {
                    null
                }
            }
        } catch (e: SecurityException) {
            throw ErrorPlataforma.Carpeta(
                contexto.getString(R.string.plataforma_carpeta_permiso_retirado),
                e,
            )
        } catch (e: RuntimeException) {
            throw ErrorPlataforma.Carpeta(
                contexto.getString(R.string.plataforma_carpeta_ilegible),
                e,
            )
        }
    }

    private fun leerBytes(uriDocumento: Uri, tope: Long, nombre: String): ByteArray {
        val flujo = try {
            contexto.contentResolver.openInputStream(uriDocumento)
        } catch (e: IOException) {
            throw ErrorPlataforma.Carpeta(
                contexto.getString(R.string.plataforma_carpeta_fichero_ilegible, nombre),
                e,
            )
        } catch (e: SecurityException) {
            throw ErrorPlataforma.Carpeta(
                contexto.getString(R.string.plataforma_carpeta_permiso_retirado),
                e,
            )
        } ?: throw ErrorPlataforma.Carpeta(
            contexto.getString(R.string.plataforma_carpeta_fichero_ilegible, nombre)
        )
        flujo.use { entrada ->
            val salida = ByteArrayOutputStream()
            val bufer = ByteArray(64 * 1024)
            var total = 0L
            while (true) {
                val leidos = try {
                    entrada.read(bufer)
                } catch (e: IOException) {
                    throw ErrorPlataforma.Carpeta(
                        contexto.getString(R.string.plataforma_carpeta_fichero_ilegible, nombre),
                        e,
                    )
                }
                if (leidos < 0) break
                total += leidos.toLong()
                if (total > tope) {
                    throw ErrorPlataforma.Carpeta(
                        contexto.getString(R.string.plataforma_carpeta_fichero_grande, nombre, tope)
                    )
                }
                salida.write(bufer, 0, leidos)
            }
            return salida.toByteArray()
        }
    }
}
