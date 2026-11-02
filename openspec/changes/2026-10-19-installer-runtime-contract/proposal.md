# Installer runtime contract and existing-file preservation

Supports Business #1411/#1390 and Gateway #466/#548. Phase R1 implementation prerequisite for the existing P0 deployment work.

The generated configuration still names an unused port, omits four baseline secrets, embeds a token in Compose, names an unqualified image and mounts state outside the runtime working directory. Init also overwrites existing files. Align templates to current startup parsing without generating credentials or claiming provisioning/readiness: current five required names, optional providers disabled, API_PORT, explicit reviewed image input and exclusive-writer persistent state/queue in /usr/app. Require the explicit .env.production interpolation file. Protect existing files and create private configuration with restrictive Unix permissions.

Keep existing serialized InstallerConfig fields for compatibility but stop advertising unused database/trust/OData settings as runtime inputs. Literal dotenv values must not be interpolated or inject extra lines; reject unsupported input without echoing it. Preserve the liveness/doctor diagnostics from #568.

Acceptance: process tests cover fresh init, required names, secret-free Compose, consistent port/state wiring, repeat-init and each preexisting-file case, symlink preservation on Unix and private permissions. Native tests cover literal/invalid inputs. Source/startup contract and policy checks remain intact; hosted Rust/fmt/Clippy/workspace checks are required because this environment has no Rust or Docker. No image is built, cloud resource provisioned, secret fetched/generated, provider enabled, live operation authorized or production acceptance claimed.
