import json
from pathlib import Path
import re
import subprocess
import sys
import unittest

from startup_secret_preflight import CONDITIONAL, GROUPED, REQUIRED, check, describe, markdown_contract

SCRIPT = Path(__file__).with_name("startup_secret_preflight.py")


class StartupPreflightTests(unittest.TestCase):
    def environment(self):
        return {key: "private-test-only-" + str(i) for i, key in enumerate(REQUIRED)}

    def test_minimum_lane_does_not_require_disabled_provider_credentials(self):
        result = check(self.environment())
        self.assertTrue(result["ready"])
        self.assertEqual(result["requiredCount"], 3)

    def test_each_fiat_provider_requires_webhook_secret_and_disabled_ignores_it(self):
        for mode in GROUPED["FIAT_WEBHOOK_SECRET"]:
            env = {**self.environment(), mode: "shadow"}
            self.assertIn("FIAT_WEBHOOK_SECRET", check(env)["missing"])
        env = {**self.environment(), "FIAT_WEBHOOK_SECRET": "sentinel_unused",
               "HMAC_SECRET": "sentinel_unused"}
        self.assertTrue(check(env)["ready"])

    def test_missing_required_secrets_fail(self):
        for key in REQUIRED:
            with self.subTest(key=key):
                env = self.environment()
                del env[key]
                self.assertEqual(check(env)["missing"], [key])
                self.assertFalse(check(env)["ready"])

    def test_blank_and_placeholder_secrets_fail(self):
        for value in ("", "  \n", "sentinel_API_TOKEN", "CONXIAN_PROD_AUTH_TOKEN_REQUIRED"):
            with self.subTest(value=value):
                self.assertFalse(check({**self.environment(), "API_TOKEN": value})["ready"])

    def test_legacy_only_token_is_accepted_and_reported_as_one_logical_requirement(self):
        env = self.environment()
        env["CONXIAN_GATEWAY_AUTH_TOKEN"] = env.pop("API_TOKEN")
        self.assertTrue(check(env)["ready"])
        self.assertEqual(check(env)["requiredCount"], 3)

    def test_invalid_explicit_canonical_token_never_falls_back_to_legacy(self):
        for value in ("", "  ", "sentinel_API_TOKEN", "CONXIAN_PROD_AUTH_TOKEN_REQUIRED"):
            env = {**self.environment(), "API_TOKEN": value,
                   "CONXIAN_GATEWAY_AUTH_TOKEN": "legacy-test-token"}
            self.assertFalse(check(env)["ready"])

    def test_legacy_token_placeholder_is_rejected(self):
        for value in ("", "sentinel_legacy", "CONXIAN_PROD_AUTH_TOKEN_REQUIRED"):
            env = self.environment()
            del env["API_TOKEN"]
            env["CONXIAN_GATEWAY_AUTH_TOKEN"] = value
            self.assertFalse(check(env)["ready"])

    def test_non_unicode_api_token_input_is_invalid_for_both_names(self):
        for key in ("API_TOKEN", "CONXIAN_GATEWAY_AUTH_TOKEN"):
            env = self.environment()
            del env["API_TOKEN"]
            env[key] = "invalid-\udcff"
            self.assertEqual(check(env)["invalid"], ["API_TOKEN"])

    def test_alias_value_never_appears_in_process_output(self):
        env = self.environment()
        env["CONXIAN_GATEWAY_AUTH_TOKEN"] = env.pop("API_TOKEN")
        result = subprocess.run([sys.executable, str(SCRIPT)], env=env,
                                capture_output=True, text=True, timeout=5, check=False)
        self.assertEqual(result.returncode, 0)
        self.assertNotIn(env["CONXIAN_GATEWAY_AUTH_TOKEN"], result.stdout + result.stderr)

    def test_all_runtime_enabled_modes_require_each_provider_secret(self):
        for mode, key in CONDITIONAL.items():
            for value in ("active", "shadow", "true", "1", "  ACTIVE "):
                with self.subTest(mode=mode, value=value):
                    env = {**self.environment(), mode: value}
                    self.assertIn(key, check(env)["missing"])
                    env[key] = "private-provider-test"
                    for grouped_key, modes in GROUPED.items():
                        if mode in modes:
                            self.assertIn(grouped_key, check(env)["missing"])
                            env[grouped_key] = "grouped-test-" + grouped_key
                    self.assertTrue(check(env)["ready"])

    def test_disabled_provider_values_are_not_read_or_reported(self):
        env = self.environment()
        for mode, key in CONDITIONAL.items():
            env[mode] = "disabled"
            env[key] = "sentinel_" + key
        self.assertTrue(check(env)["ready"])

    def test_webhook_domains_cannot_share_a_secret(self):
        env = self.environment()
        env["STITCH_MODE"] = "active"
        env["FIAT_WEBHOOK_SECRET"] = "shared-test-value"
        env["SETTLEMENT_INGRESS_SECRET"] = " shared-test-value "
        self.assertEqual(check(env)["invalid"], ["FIAT_WEBHOOK_SECRET", "SETTLEMENT_INGRESS_SECRET"])

    def test_subprocess_output_contains_no_value_fingerprint_or_extra_environment(self):
        env = self.environment()
        env["UNRELATED_SECRET"] = "never-disclose-this-unrelated-value"
        env["FIAT_WEBHOOK_SECRET"] = "webhook-test-value"
        env["RAMP_MODE"] = "active"
        env["RAMP_API_KEY"] = "arbitrary-test-value\nwith-log-injection"
        result = subprocess.run([sys.executable, str(SCRIPT)], env=env, capture_output=True,
                                text=True, timeout=5, check=False)
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stderr, "")
        self.assertTrue(json.loads(result.stdout)["ready"])
        for value in env.values():
            self.assertNotIn(value, result.stdout)
        self.assertNotIn("UNRELATED_SECRET", result.stdout)

    def test_empty_environment_cli_fails_with_only_known_names(self):
        result = subprocess.run([sys.executable, str(SCRIPT)], env={}, capture_output=True,
                                text=True, timeout=5, check=False)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(json.loads(result.stdout)["missing"], sorted(REQUIRED))
        self.assertEqual(result.stderr, "")

    def test_preflight_is_bound_to_current_rust_startup_contract(self):
        source = (SCRIPT.parents[1] / "cmd/gateway/src/config.rs").read_text()
        startup = source.split("pub fn from_env() -> Self {", 1)[1].split("\n#[cfg(test)]", 1)[0]
        required = set(re.findall(r'Self::get_mandatory_env\(\s*"([A-Z0-9_]+)"', startup))
        self.assertRegex(startup, r"let\s+api_token\s*=\s*resolve_api_token_with\(")
        required.add("API_TOKEN")
        self.assertEqual(required, set(REQUIRED) | set(CONDITIONAL.values()) | set(GROUPED))
        bindings = dict(re.findall(r'if Self::provider_enabled\("([A-Z0-9_]+)"\)\s*\{\s*Some\(Self::get_mandatory_env\(\s*"([A-Z0-9_]+)"', startup))
        self.assertEqual(bindings, CONDITIONAL)

    def test_describe_never_inspects_environment_or_grants_readiness(self):
        from unittest.mock import patch
        with patch("startup_secret_preflight.os.environ", None):
            contract = describe()
        self.assertEqual(contract["minimumRequiredCount"], len(REQUIRED))
        self.assertEqual(contract["maximumConditionalRequiredCount"], len(REQUIRED) + len(CONDITIONAL) + len(GROUPED))
        self.assertFalse(contract["environmentInspected"])
        self.assertFalse(contract["runtimeReadinessEstablished"])
        result = subprocess.run([sys.executable, str(SCRIPT), "--describe"],
                                env={"API_TOKEN": "private-contract-test-value"},
                                capture_output=True, text=True, timeout=5, check=False)
        self.assertEqual(result.returncode, 0)
        self.assertEqual(json.loads(result.stdout), contract)
        self.assertNotIn("private-contract-test-value", result.stdout + result.stderr)

    def test_every_enabled_provider_combination_has_the_correct_count(self):
        import itertools
        for enabled in itertools.product((False, True), repeat=len(CONDITIONAL)):
            env = self.environment()
            for (mode, key), active in zip(CONDITIONAL.items(), enabled):
                env[mode] = "active" if active else "disabled"
                if active:
                    env[key] = "provider-test-value"
            grouped_count = 0
            for key, modes in GROUPED.items():
                if any(env.get(mode) == "active" for mode in modes):
                    env[key] = "grouped-test-" + key
                    grouped_count += 1
            result = check(env)
            self.assertTrue(result["ready"])
            self.assertEqual(result["requiredCount"], len(REQUIRED) + sum(enabled) + grouped_count)

    def test_readme_contract_is_generated_from_the_runtime_bound_preflight(self):
        readme = (SCRIPT.parents[1] / "README.md").read_text()
        begin, end = "<!-- STARTUP-SECRET-CONTRACT:BEGIN -->", "<!-- STARTUP-SECRET-CONTRACT:END -->"
        self.assertEqual(readme.count(begin), 1)
        self.assertEqual(readme.count(end), 1)
        block = begin + readme.split(begin, 1)[1].split(end, 1)[0] + end
        self.assertEqual(block, markdown_contract())
        self.assertNotRegex(readme, r"(?i)(?:~?11|eleven)\s+(?:owner\s+)?secrets")

    def test_blueprint_requests_only_the_minimum_startup_secret_set(self):
        blueprint = (SCRIPT.parents[1] / "render.yaml").read_text()
        prompts = set(re.findall(r'- key: ([A-Z0-9_]+)\n\s+sync: false', blueprint))
        self.assertEqual(prompts, set(REQUIRED))
        self.assertIn("healthCheckPath: /api/v1/health", blueprint)
        for mode in CONDITIONAL:
            self.assertRegex(blueprint, rf'- key: {mode}\n\s+value: disabled')


if __name__ == "__main__":
    unittest.main()
