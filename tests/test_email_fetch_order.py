"""Tests for email fetch ordering and non-spam counting.

The IMAP fetcher must:
1. Return emails newest-first
2. Only count non-spam, non-empty emails toward the limit
3. Skip spam without reducing the effective limit
"""
import email
import sys
import os
from unittest.mock import MagicMock, patch
from email.message import EmailMessage

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def _make_raw_email(from_addr, subject="Test", body="This is a valid email body with enough content to pass filters.", to_addr="user@example.com"):
    """Build raw RFC822 bytes for a fake email."""
    msg = EmailMessage()
    msg["From"] = from_addr
    msg["To"] = to_addr
    msg["Subject"] = subject
    msg["Date"] = "Thu, 13 Mar 2026 10:00:00 +0000"
    msg["Message-ID"] = f"<{from_addr}-{subject}@test>"
    msg.set_content(body)
    return msg.as_bytes()


def _make_spam_email(from_addr="newsletter@spamco.com"):
    """Build raw RFC822 bytes for a spam email."""
    return _make_raw_email(from_addr, subject="Buy now!", body="Amazing deals inside! Click here for savings!")


def _make_short_email(from_addr="someone@example.com"):
    """Build raw RFC822 bytes for an email with body too short."""
    return _make_raw_email(from_addr, subject="Hi", body="ok")


def _mock_imap(emails_data):
    """Create a mock IMAP connection that returns the given emails.

    emails_data: list of raw email bytes, index 0 = oldest, last = newest
    (matching IMAP message number ordering where higher = newer)
    """
    imap = MagicMock()
    imap.starttls.return_value = None
    imap.login.return_value = ("OK", [])
    imap.list.return_value = ("OK", [b'(\\HasNoChildren) "/" "INBOX"'])
    imap.select.return_value = ("OK", [b"1"])

    # Message numbers: b"1 2 3 ..." (1=oldest, N=newest)
    nums = " ".join(str(i + 1) for i in range(len(emails_data)))
    imap.search.return_value = ("OK", [nums.encode()])

    def fake_fetch(num, parts):
        idx = int(num) - 1
        if 0 <= idx < len(emails_data):
            return ("OK", [(b"1 (RFC822 {1234})", emails_data[idx])])
        return ("OK", [(None,)])

    imap.fetch.side_effect = fake_fetch
    imap.logout.return_value = None
    return imap


def test_spam_not_counted_toward_limit():
    """Spam emails must not reduce the effective limit."""
    from learn_email_imap import fetch_imap_emails

    # Build 8 emails: indices 0-2 valid, 3-5 spam, 6-7 valid
    emails_data = [
        _make_raw_email("alice@bank.com", "Statement 1"),
        _make_raw_email("bob@property.com", "Lease renewal"),
        _make_raw_email("carol@finance.com", "Investment update"),
        _make_spam_email("newsletter@spamco.com"),
        _make_spam_email("marketing@deals.com"),
        _make_spam_email("newsletter@offers.com"),
        _make_raw_email("dave@realty.com", "Property closing"),
        _make_raw_email("eve@bank.com", "Wire confirmation"),
    ]

    account_info = {
        "email": "user@example.com",
        "agent": "test-agent",
        "env_user": "IMAP_USERNAME",
        "env_pass": "IMAP_PASSWORD",
    }
    env = {"IMAP_USERNAME": "user", "IMAP_PASSWORD": "pass"}

    mock_imap = _mock_imap(emails_data)
    with patch("learn_email_imap.imaplib.IMAP4", return_value=mock_imap):
        result = fetch_imap_emails("account-1", account_info, env, limit=5)

    # Should get exactly 5 valid emails, spam doesn't count
    assert len(result) == 5, f"Expected 5 valid emails, got {len(result)}"


def test_fetch_newest_first():
    """Emails must be returned newest-first (last message number first)."""
    from learn_email_imap import fetch_imap_emails

    emails_data = [
        _make_raw_email("oldest@test.com", "Email 1 oldest"),
        _make_raw_email("middle@test.com", "Email 2 middle"),
        _make_raw_email("newest@test.com", "Email 3 newest"),
    ]

    account_info = {
        "email": "user@example.com",
        "agent": "test-agent",
        "env_user": "IMAP_USERNAME",
        "env_pass": "IMAP_PASSWORD",
    }
    env = {"IMAP_USERNAME": "user", "IMAP_PASSWORD": "pass"}

    mock_imap = _mock_imap(emails_data)
    with patch("learn_email_imap.imaplib.IMAP4", return_value=mock_imap):
        result = fetch_imap_emails("account-1", account_info, env, limit=10)

    assert len(result) == 3
    # Newest (msg 3) should be first in results
    assert "newest@test.com" in result[0]["from_addr"]
    assert "oldest@test.com" in result[-1]["from_addr"]


def test_short_body_not_counted_toward_limit():
    """Emails with body too short must not count toward the limit."""
    from learn_email_imap import fetch_imap_emails

    emails_data = [
        _make_raw_email("valid1@test.com", "Valid 1"),
        _make_short_email("short1@test.com"),
        _make_short_email("short2@test.com"),
        _make_raw_email("valid2@test.com", "Valid 2"),
        _make_raw_email("valid3@test.com", "Valid 3"),
    ]

    account_info = {
        "email": "user@example.com",
        "agent": "test-agent",
        "env_user": "IMAP_USERNAME",
        "env_pass": "IMAP_PASSWORD",
    }
    env = {"IMAP_USERNAME": "user", "IMAP_PASSWORD": "pass"}

    mock_imap = _mock_imap(emails_data)
    with patch("learn_email_imap.imaplib.IMAP4", return_value=mock_imap):
        result = fetch_imap_emails("account-1", account_info, env, limit=3)

    assert len(result) == 3, f"Expected 3 valid emails, got {len(result)}"
    addrs = [e["from_addr"] for e in result]
    assert all("short" not in a for a in addrs), f"Short-body emails leaked through: {addrs}"


def test_filter_to_separates_accounts():
    """Emails to account-2 must not appear in account-1 filtered results."""
    from learn_email_imap import fetch_imap_emails

    emails_data = [
        _make_raw_email("alice@bank.com", "For account 1", to_addr="user@example.com"),
        _make_raw_email("bob@prop.com", "For account 2", to_addr="user2@example.com"),
        _make_raw_email("carol@bank.com", "Also account 1", to_addr="user@example.com"),
        _make_raw_email("dave@prop.com", "Also account 2", to_addr="user2@example.com"),
    ]

    account_1 = {
        "email": "user@example.com",
        "agent": "test-agent",
        "env_user": "IMAP_USERNAME",
        "env_pass": "IMAP_PASSWORD",
        "filter_to": "user@example.com",
    }
    env = {"IMAP_USERNAME": "user", "IMAP_PASSWORD": "pass"}

    mock_imap = _mock_imap(emails_data)
    with patch("learn_email_imap.imaplib.IMAP4", return_value=mock_imap):
        result = fetch_imap_emails("account-1", account_1, env, limit=10)

    assert len(result) == 2, f"Expected 2 account-1 emails, got {len(result)}"

    account_2 = {
        "email": "user2@example.com",
        "agent": "test-agent",
        "env_user": "IMAP_USERNAME",
        "env_pass": "IMAP_PASSWORD",
        "filter_to": "user2@example.com",
    }
    mock_imap2 = _mock_imap(emails_data)
    with patch("learn_email_imap.imaplib.IMAP4", return_value=mock_imap2):
        result2 = fetch_imap_emails("account-2", account_2, env, limit=10)

    assert len(result2) == 2, f"Expected 2 account-2 emails, got {len(result2)}"
