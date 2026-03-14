"""Tests for email spam filter in learn_email_imap.py.

Financial/property emails must NOT be filtered as spam.
"""
from email.message import EmailMessage


def test_bank_emails_not_filtered_as_spam():
    """Bank/financial emails must NOT be filtered as spam."""
    from learn_email_imap import is_spam_or_ad

    msg = EmailMessage()
    msg["From"] = "no-reply@alertsp.chase.com"
    msg["List-Unsubscribe"] = "<mailto:unsub@chase.com>"
    assert not is_spam_or_ad(msg, "no-reply@alertsp.chase.com"), \
        "Chase bank alert filtered as spam"

    msg2 = EmailMessage()
    msg2["From"] = "noreply@bankofamerica.com"
    assert not is_spam_or_ad(msg2, "noreply@bankofamerica.com"), \
        "BofA alert filtered as spam"


def test_property_management_not_filtered():
    """Property management notifications must NOT be filtered."""
    from learn_email_imap import is_spam_or_ad

    msg = EmailMessage()
    msg["From"] = "notifications@stonelink.com"
    assert not is_spam_or_ad(msg, "notifications@stonelink.com"), \
        "Property management notification filtered as spam"


def test_financial_services_not_filtered():
    """Financial service emails with List-Unsubscribe must NOT be filtered."""
    from learn_email_imap import is_spam_or_ad

    msg = EmailMessage()
    msg["From"] = "alerts@verizon.com"
    msg["List-Unsubscribe"] = "<https://unsub.verizon.com>"
    assert not is_spam_or_ad(msg, "alerts@verizon.com"), \
        "Verizon bill filtered as spam"


def test_actual_spam_still_filtered():
    """Real spam/marketing emails must be filtered."""
    from learn_email_imap import is_spam_or_ad

    msg = EmailMessage()
    msg["From"] = "newsletter@randomcompany.com"
    assert is_spam_or_ad(msg, "newsletter@randomcompany.com")

    msg2 = EmailMessage()
    msg2["From"] = "marketing@promo-deals.com"
    assert is_spam_or_ad(msg2, "marketing@promo-deals.com")

    msg3 = EmailMessage()
    msg3["From"] = "mailer-daemon@server.com"
    assert is_spam_or_ad(msg3, "mailer-daemon@server.com")
