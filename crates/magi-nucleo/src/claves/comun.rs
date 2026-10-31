//! Piezas comunes de las conversiones de claves del móvil (Fase 11): el
//! lector mínimo de DER, el formato de cadena SSH y el vector fijo de pruebas.

/// Lector de estructuras DER sobre una porción de bytes.
///
/// Solo se admiten las estructuras que producen el Keystore y OpenSSL:
/// longitudes definidas y canónicas, sin etiquetas de contexto.
pub(super) struct Lector<'a> {
    datos: &'a [u8],
    posicion: usize,
}

impl<'a> Lector<'a> {
    /// Empieza a leer por el principio de `datos`.
    pub(super) fn nuevo(datos: &'a [u8]) -> Self {
        Self { datos, posicion: 0 }
    }

    /// Comprueba que no quedan datos sin leer.
    pub(super) fn terminar(&self, contexto: &str) -> Result<(), String> {
        if self.posicion == self.datos.len() {
            Ok(())
        } else {
            Err(format!("{contexto}: datos sobrantes"))
        }
    }

    /// Lee un valor con la etiqueta esperada y devuelve su contenido.
    pub(super) fn leer(&mut self, etiqueta: u8, contexto: &str) -> Result<&'a [u8], String> {
        let Some(leida) = self.datos.get(self.posicion).copied() else {
            return Err(format!("{contexto}: faltan datos"));
        };
        if leida != etiqueta {
            return Err(format!(
                "{contexto}: se esperaba la etiqueta 0x{etiqueta:02x} y llegó 0x{leida:02x}"
            ));
        }
        self.posicion += 1;
        let longitud = self.longitud(contexto)?;
        let fin = self
            .posicion
            .checked_add(longitud)
            .ok_or_else(|| format!("{contexto}: longitud desbordada"))?;
        if fin > self.datos.len() {
            return Err(format!("{contexto}: longitud fuera de los datos"));
        }
        let contenido = &self.datos[self.posicion..fin];
        self.posicion = fin;
        Ok(contenido)
    }

    /// Lee una longitud DER definida y canónica.
    fn longitud(&mut self, contexto: &str) -> Result<usize, String> {
        let Some(primero) = self.datos.get(self.posicion).copied() else {
            return Err(format!("{contexto}: falta la longitud"));
        };
        self.posicion += 1;
        if primero & 0x80 == 0 {
            return Ok(usize::from(primero));
        }
        let bytes = usize::from(primero & 0x7f);
        if bytes == 0 || bytes > 4 {
            return Err(format!("{contexto}: longitud DER no admitida"));
        }
        let fin = self
            .posicion
            .checked_add(bytes)
            .ok_or_else(|| format!("{contexto}: longitud desbordada"))?;
        if fin > self.datos.len() {
            return Err(format!("{contexto}: falta la longitud"));
        }
        if self.datos[self.posicion] == 0 {
            return Err(format!("{contexto}: longitud DER no canónica"));
        }
        let mut longitud = 0usize;
        for _ in 0..bytes {
            longitud = (longitud << 8) | usize::from(self.datos[self.posicion]);
            self.posicion += 1;
        }
        if longitud < 128 {
            return Err(format!("{contexto}: longitud DER no canónica"));
        }
        Ok(longitud)
    }
}

/// Algoritmo de la clave y de la firma del móvil.
pub(crate) const ALGORITMO: &str = "ecdsa-sha2-nistp256";

/// Curva de la clave del móvil.
pub(crate) const CURVA: &str = "nistp256";

/// Escribe una cadena SSH: longitud u32 big-endian y los bytes.
///
/// Es también el formato de un mpint positivo, así que vale para los dos.
pub(super) fn escribir_cadena(salida: &mut Vec<u8>, datos: &[u8]) {
    let longitud = u32::try_from(datos.len()).expect("una cadena SSH no pasa de 4 GiB");
    salida.extend_from_slice(&longitud.to_be_bytes());
    salida.extend_from_slice(datos);
}

#[cfg(test)]
pub(super) mod vectores {
    //! Vector fijo generado con OpenSSL (P-256) para las pruebas de claves.
    //! La línea OpenSSH lleva el comentario `magi@prueba`.

    /// Clave pública SPKI (DER) de P-256.
    pub(crate) const SPKI_HEX: &str = "3059301306072a8648ce3d020106082a8648ce3d03010703420004e23140c5824a2d853d54e154850ae8a208d77c054faa3a3ca5dbc9d2138ca825a6b2bb22d522b0812fd5e15fb94ac3c0859baec69c03b23225b97afcbe96229e";

    /// Firma DER (SEQUENCE de dos INTEGER) de `MENSAJE` con esa clave.
    pub(crate) const FIRMA_DER_HEX: &str = "30460221008482ca7bc2193a0c1092b8b65d22e1446abe7cb2c101d1f87d8dc7093411d677022100e294000b343f991f03e85da2b3c827cbc57275e7d97122b9286bc4db6db80b1c";

    /// Blob de firma SSH que debe salir de la firma DER.
    pub(crate) const FIRMA_SSH_HEX: &str = "0000001365636473612d736861322d6e697374703235360000004a00000021008482ca7bc2193a0c1092b8b65d22e1446abe7cb2c101d1f87d8dc7093411d6770000002100e294000b343f991f03e85da2b3c827cbc57275e7d97122b9286bc4db6db80b1c";

    /// Línea OpenSSH de la clave, con el comentario.
    pub(crate) const LINEA: &str = "ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBOIxQMWCSi2FPVThVIUK6KII13wFT6o6PKXbydITjKglprK7ItUisIEv1eFfuUrDwIWbrsacA7IyJbl6/L6WIp4= magi@prueba";

    /// Mensaje firmado con SHA-256.
    pub(crate) const MENSAJE: &[u8] = b"MAGI: mensaje de prueba para la firma ECDSA";

    /// Decodifica un texto hexadecimal.
    pub(crate) fn de_hex(texto: &str) -> Vec<u8> {
        assert!(texto.len() % 2 == 0, "el hexadecimal tiene longitud impar");
        let mut bytes = Vec::with_capacity(texto.len() / 2);
        let mut indice = 0;
        while indice < texto.len() {
            let par = &texto[indice..indice + 2];
            bytes.push(u8::from_str_radix(par, 16).expect("hexadecimal válido"));
            indice += 2;
        }
        bytes
    }
}
