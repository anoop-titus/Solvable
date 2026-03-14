"""Tests for IMAP credential configuration.

These verify that IMAP accounts from config reference the correct
env vars for credentials.
"""
import sys
import os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def test_imap_accounts_have_credential_env_vars():
    """Each IMAP account must reference env_user and env_pass fields."""
    from learner_config import get_imap_config

    imap_cfg = get_imap_config()
    accounts = imap_cfg.get("accounts", {})

    for account_key, info in accounts.items():
        assert "env_user" in info, f"{account_key}: missing 'env_user' field"
        assert "env_pass" in info, f"{account_key}: missing 'env_pass' field"
        assert info["env_user"], f"{account_key}: env_user is empty"
        assert info["env_pass"], f"{account_key}: env_pass is empty"


def test_imap_accounts_have_valid_agent():
    """Each IMAP account must map to an agent name."""
    from learner_config import get_imap_config

    imap_cfg = get_imap_config()
    accounts = imap_cfg.get("accounts", {})

    for account_key, info in accounts.items():
        assert "agent" in info, f"{account_key}: missing 'agent' field"
        assert info["agent"], f"{account_key}: agent is empty"


def test_imap_accounts_have_email_field():
    """Each IMAP account must have a valid email address."""
    from learner_config import get_imap_config

    imap_cfg = get_imap_config()
    accounts = imap_cfg.get("accounts", {})

    for account_key, info in accounts.items():
        assert "email" in info, f"{account_key}: missing 'email' field"
        assert "@" in info["email"], f"{account_key}: invalid email '{info['email']}'"
