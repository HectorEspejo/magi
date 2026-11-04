//! Servidor HTTP y WebSocket del visor (pista servidor del visor): solo en
//! 127.0.0.1, arranca con la primera sesión de escritorio y se para con la
//! última. Enlaces de un solo uso de 256 bits y 60 s, clave por sesión,
//! comprobación de Host y Origin, CSP estricta y ficheros del visor incluidos
//! en el binario.
