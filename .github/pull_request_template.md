<!-- Castellano arriba, English below. Lee CONTRIBUTING.md / Read CONTRIBUTING.en.md. -->

## Qué cambia y por qué

<!-- Una o dos frases. Enlaza la issue o la propuesta si la hay. -->

## Lista de comprobación

- [ ] Hay pruebas del cambio, y `TZ=UTC cargo test --workspace --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all --check` y `cargo deny check` pasan.
- [ ] Si cambia algo visible para el usuario, hay una línea en «Sin publicar» de `CHANGELOG.md` (o de `android/CHANGELOG.md`).
- [ ] El manual (`docs/manual/`) está al día; si cambia el README o `CONTRIBUTING`, también su versión en inglés.
- [ ] No hay datos reales: ni IP, ni nombres de host, ni rutas `/home/<nombre>`, ni correos, ni claves (`repositorio_publico` pasa).
- [ ] El código, los comentarios y los textos de la interfaz están en castellano.

---

## What changes and why

<!-- One or two sentences. Link the issue or proposal if there is one. -->

## Checklist

- [ ] The change has tests, and `TZ=UTC cargo test --workspace --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all --check` and `cargo deny check` pass.
- [ ] If something user-visible changes, there is a line under "Sin publicar" in `CHANGELOG.md` (or `android/CHANGELOG.md`).
- [ ] The manual (`docs/manual/`) is up to date; if the README or `CONTRIBUTING` changes, so does its English version.
- [ ] There is no real data: no IPs, hostnames, `/home/<name>` paths, emails or keys (`repositorio_publico` passes).
- [ ] Code, comments and interface texts are in Spanish.
