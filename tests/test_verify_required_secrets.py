from pathlib import Path
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import verify_required_secrets as guard

SOURCE = '\n'.join(f'get_mandatory_env("{key}", SENTINEL)' for key in
                   ("API_TOKEN", "RAMP_API_KEY", "ALCHEMY_PAY_APP_ID", "INFOBIP_API_KEY"))


class RequiredSecretsTests(unittest.TestCase):
    def test_only_explicit_literal_disabled_render_modes_relax_provider_requirements(self):
        for value in ("disabled", '"disabled"', "'disabled'"):
            with self.subTest(value=value):
                text = "      - key: RAMP_MODE\n        value: " + value + "\n"
                keys = guard.required_keys(SOURCE, text, "render.yaml")
                self.assertNotIn("RAMP_API_KEY", keys)
                self.assertIn("API_TOKEN", keys)
                self.assertIn("INFOBIP_API_KEY", keys)

    def test_unknown_active_shadow_and_dynamic_modes_require_the_secret(self):
        for value in ("active", "shadow", "true", "1", "unknown", "${RAMP_MODE:-disabled}"):
            with self.subTest(value=value):
                text = "      - key: RAMP_MODE\n        value: " + value + "\n"
                self.assertIn("RAMP_API_KEY", guard.required_keys(SOURCE, text, "render.yaml"))

    def test_absent_and_duplicate_modes_keep_the_requirement(self):
        block = "      - key: RAMP_MODE\n        value: disabled\n"
        for text in ("", block + block, block + "      - key: RAMP_MODE\n        value: active\n"):
            self.assertIn("RAMP_API_KEY", guard.required_keys(SOURCE, text, "render.yaml"))

    def test_compose_and_new_unknown_keys_are_not_exempted(self):
        text = "      - key: RAMP_MODE\n        value: disabled\n"
        self.assertIn("RAMP_API_KEY", guard.required_keys(SOURCE, text, "docker-compose.yml"))
        source = SOURCE + '\nget_mandatory_env("NEW_SECRET", SENTINEL)'
        self.assertIn("NEW_SECRET", guard.required_keys(source, text, "render.yaml"))

    def test_missing_source_is_a_failure(self):
        with patch.object(guard, "CONFIG_RS", Path("/nonexistent/conxian-config.rs")):
            self.assertEqual(guard.main(), 1)

    def test_actual_blueprint_and_runtime_agree(self):
        source = guard.CONFIG_RS.read_text()
        blueprint = (ROOT / "render.yaml").read_text()
        applicable = guard.required_keys(source, blueprint, "render.yaml")
        self.assertEqual(applicable, {"API_TOKEN", "FIAT_WEBHOOK_SECRET",
                                     "SETTLEMENT_INGRESS_SECRET", "HMAC_SECRET",
                                     "OFFLINE_QUEUE_SECRET"})
        self.assertLessEqual(applicable, guard.declared_keys(blueprint, "render.yaml"))

    def test_api_alias_does_not_replace_canonical_deploy_requirement(self):
        source = guard.CONFIG_RS.read_text()
        self.assertIn("API_TOKEN", guard.mandatory_keys(source))
        self.assertNotIn("CONXIAN_GATEWAY_AUTH_TOKEN", guard.mandatory_keys(source))


if __name__ == "__main__":
    unittest.main()
