{
  lib,
  rustPlatform,
}:
let
  cargoToml = lib.importTOML ./Cargo.toml;
in
rustPlatform.buildRustPackage (finalAttrs: {
  pname = cargoToml.package.name;
  version = cargoToml.package.version;

  src = ./.;

  cargoLock.lockFile = ./Cargo.lock;

  meta = {
    description = "Syncing the TAM intranet timetable to a caldav server";
    homepage = "https://github.com/jullanggit/intranet-caldav-sync";
    license = lib.licenses.agpl3Only;
    maintainers = with lib.maintainers; [ jullanggit ];
    mainProgram = cargoToml.package.name;
  };
})
