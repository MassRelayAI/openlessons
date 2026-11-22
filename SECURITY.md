# Security and safety

## Reporting

Please report privately through GitHub's private vulnerability reporting: open the **Security** tab of this repository and
choose **Report a vulnerability** (https://github.com/MassRelayAI/openlessons/security/advisories/new). Do not open a public
issue for a security problem or for content that could harm a child.

Say what you found, where (file and line, or lesson id), and how to reproduce it. You will get an acknowledgement within a
few working days.

## What we want to hear about

**Content**

- anything unsafe or unsuitable for a child in a lesson, an item, a hint or feedback;
- personal data of a real person anywhere in the repository;
- text or media copied from a source whose licence does not allow it, including a standard's official wording beyond its
  recorded terms;
- an answer key that is wrong in a `published` record.

**Tooling**

- a way to make a failing gate pass: a bypass of the self-test, a record the validator misreads as clean, a path that
  escapes the tree;
- a way for content to reach `reviewed` or `published` without a person's review, or for a reviewed record to change
  without the base comparison noticing;
- a learner export (`export --format learner`) that carries an answer;
- a problem in the CI workflow (the workflow uses a read-only token and actions pinned by commit).

`openlessons` reads files and prints verdicts. It opens no network socket. It starts one other program, `git`, and only
when asked to compare with a base revision (`--base`).

## Supported versions

The latest `main`. This is an early project (0.x); fixes land on `main`.
