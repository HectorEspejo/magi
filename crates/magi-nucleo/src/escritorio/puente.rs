//! Puente entre los eventos de una fuente (RDP o VNC) y los mensajes del
//! visor (pista servidor del visor, D225): agrupa las zonas sucias a 60 fps,
//! manda rectángulos en bruto, copias y el H.264 de Windows tal cual, mantiene
//! el búfer del escritorio para repintar y aplica contrapresión.
