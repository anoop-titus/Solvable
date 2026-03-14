"""Tests for parallel task generation in the learn orchestrator.

These verify that `learn -p -a` spawns the correct mix of
IMAP + Airtable email processes alongside Dropbox.
"""
import importlib
import sys
import os


def _import_learn():
    """Import the 'learn' script as a module (it has no .py extension)."""
    learn_path = os.path.join(
        os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "learn"
    )
    loader = importlib.machinery.SourceFileLoader("learn_cli", learn_path)
    spec = importlib.util.spec_from_loader("learn_cli", loader)
    mod = importlib.util.module_from_spec(spec)
    sys.modules["learn_cli"] = mod
    spec.loader.exec_module(mod)
    return mod


def test_parallel_aggressive_includes_imap_and_airtable():
    """learn -p -a must spawn IMAP + Airtable email tasks."""
    mod = _import_learn()
    tasks = mod.build_task_list(dropbox=True, email=True, aggressive=True)
    labels = [t[0] for t in tasks]

    # Should have at least IMAP tasks if accounts are configured
    imap_cfg = mod._imap_accounts
    if imap_cfg:
        assert any("email-imap" in l for l in labels), "No IMAP email tasks"

    # Should have Airtable tasks if sender groups are configured
    sender_groups = mod._sender_groups
    for group_name in sender_groups:
        assert any(f"email-airtable/{group_name}" in l for l in labels), f"No {group_name} Airtable task"


def test_parallel_email_only_no_dropbox():
    """learn -p --email must not include dropbox tasks."""
    mod = _import_learn()
    tasks = mod.build_task_list(dropbox=False, email=True, aggressive=False)
    labels = [t[0] for t in tasks]

    assert not any("dropbox" in l for l in labels), "Dropbox tasks found when email-only"


def test_imap_not_multiplied_in_aggressive():
    """IMAP should spawn 1 process per account, not multiplied by aggressive."""
    mod = _import_learn()
    tasks = mod.build_task_list(dropbox=False, email=True, aggressive=True)
    imap_tasks = [t for t in tasks if "email-imap" in t[0]]

    # Should match configured account count, NOT multiplied
    expected = len(mod._imap_accounts)
    assert len(imap_tasks) == expected, f"Expected {expected} IMAP tasks, got {len(imap_tasks)}"


def test_dropbox_multiplied_in_aggressive():
    """Dropbox should be multiplied 3x in aggressive mode."""
    mod = _import_learn()
    tasks = mod.build_task_list(dropbox=True, email=False, aggressive=True)
    dropbox_tasks = [t for t in tasks if "dropbox" in t[0]]

    # folders * 3 multiplier
    expected = len(mod.FOLDERS) * 3
    assert len(dropbox_tasks) == expected, f"Expected {expected} Dropbox tasks, got {len(dropbox_tasks)}"
