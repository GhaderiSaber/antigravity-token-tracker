from setuptools import setup, find_packages

setup(
    name="antigravity-token-tracker",
    version="1.0.0",
    description="Multi-account weekly token and quota lifecycle monitor for Google Antigravity",
    author="Google Antigravity Pair",
    packages=find_packages(),
    install_requires=[
        "rich>=13.0.0",
    ],
    entry_points={
        "console_scripts": [
            "agy-token = antigravity_tracker.cli:main",
        ],
    },
    python_requires=">=3.8",
)
