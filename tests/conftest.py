import sys
import os
import pytest

# Add parent dir to path so we can import learn modules
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


@pytest.fixture
def real_env():
    """Load real .env for credential validation tests."""
    from learn_common import load_env
    return load_env()
