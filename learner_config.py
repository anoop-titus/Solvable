#!/usr/bin/env python3
"""
Configuration loader for the Learner Agent system.
Reads config.yaml (falling back to config.example.yaml) and provides
accessor functions for all configurable values.
"""
import os
import yaml

_config_cache = None


def get_learner_home():
    """Return the base directory for the learner agent.
    Uses LEARNER_HOME env var if set, otherwise the directory containing this script."""
    return os.environ.get("LEARNER_HOME", os.path.dirname(os.path.abspath(__file__)))


def load_config():
    """Load and return the parsed config dict from config.yaml (or config.example.yaml)."""
    global _config_cache
    if _config_cache is not None:
        return _config_cache

    home = get_learner_home()
    config_path = os.path.join(home, "config.yaml")
    if not os.path.exists(config_path):
        config_path = os.path.join(home, "config.example.yaml")
    if not os.path.exists(config_path):
        raise FileNotFoundError(
            f"No config.yaml or config.example.yaml found in {home}. "
            "Run: cp config.example.yaml config.yaml"
        )
    with open(config_path) as f:
        _config_cache = yaml.safe_load(f)
    return _config_cache


def get_db_path():
    """Return path to the learnings SQLite database."""
    return os.path.join(get_learner_home(), "db", "learnings.db")


def get_env_path():
    """Return path to the .env file."""
    return os.path.join(get_learner_home(), ".env")


def get_model():
    """Return the LLM model string from config."""
    cfg = load_config()
    return cfg.get("model", "openrouter/google/gemini-2.5-flash")


def get_chunk_size():
    """Return the chunk size for splitting large documents."""
    cfg = load_config()
    return cfg.get("chunk_size", 100_000)


def get_imap_config():
    """Return IMAP configuration dict with host, port, starttls, accounts."""
    cfg = load_config()
    imap = cfg.get("imap", {})
    return {
        "host": imap.get("host", "127.0.0.1"),
        "port": imap.get("port", 993),
        "starttls": imap.get("starttls", False),
        "accounts": imap.get("accounts", {}),
    }


def get_dropbox_folders():
    """Return list of {agent, path} dicts for Dropbox folders."""
    cfg = load_config()
    return cfg.get("dropbox", {}).get("folders", [])


def get_agent_prompt(agent_name):
    """Return the prompt string for a given agent, or a sensible default."""
    cfg = load_config()
    agents = cfg.get("agents", {})
    agent_cfg = agents.get(agent_name, {})
    if isinstance(agent_cfg, dict):
        prompt = agent_cfg.get("prompt", "")
        if prompt:
            return prompt.strip()
    # Default prompt
    return (
        "You are a learning agent extracting knowledge from documents. "
        "Focus on: key decisions, action items, deadlines, financial figures, "
        "relationships, and strategic insights.\n\n"
        "Extract 3-8 specific, actionable learnings from the following content. "
        "Include names, dates, amounts, and terms where present. "
        "Return ONLY a JSON array of strings: [\"learning 1\", \"learning 2\", ...]\n"
        "IMPORTANT: Always respond in English only."
    )


def get_airtable_config():
    """Return Airtable configuration dict with account_map and sender_groups."""
    cfg = load_config()
    at = cfg.get("airtable", {})
    return {
        "account_map": at.get("account_map", {}),
        "sender_groups": at.get("sender_groups", {}),
    }


def get_gws_bin():
    """Return path to gws binary, or None if not configured."""
    cfg = load_config()
    return cfg.get("gws_bin", None)


def get_research_config():
    """Return research integration config dict with script and db paths."""
    cfg = load_config()
    return {
        "script": cfg.get("research_script"),
        "db": cfg.get("research_db"),
    }


def get_configured_agents():
    """Return list of agent names from config."""
    cfg = load_config()
    return list(cfg.get("agents", {}).keys())


def get_configured_account_keys():
    """Return list of IMAP account keys from config."""
    imap_cfg = get_imap_config()
    return list(imap_cfg.get("accounts", {}).keys())


def get_configured_sender_groups():
    """Return sender_groups dict from airtable config."""
    at = get_airtable_config()
    return at.get("sender_groups", {})
