"""The agent's 2.4 GHz link record: only changes are written, with what failed."""

from ak980 import agent


class FakeDongle:
    answers: list = []

    def __init__(self, info):
        pass

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        pass

    @staticmethod
    def find():
        return object()

    def read_battery(self):
        answer = self.answers.pop(0)
        if isinstance(answer, Exception):
            raise answer
        return answer


def test_link_log_records_changes_only(monkeypatch, tmp_path):
    monkeypatch.setattr(agent, "LINK_LOG", str(tmp_path / "link.log"))
    monkeypatch.setattr(agent, "Dongle", FakeDongle)
    FakeDongle.answers = [80, 79, None, None, TimeoutError(110, "Connection timed out"), 81]
    a, state = agent.Agent(), None
    for _ in range(6):
        state = a._check_link(state)
    lines = [line.split(" ", 2)[2] for line in (tmp_path / "link.log").read_text().splitlines()]
    assert lines == [
        "2.4 GHz: the keyboard answers, battery 80%",
        "2.4 GHz: the dongle took the request, the keyboard did not answer",
        "2.4 GHz: the dongle did not take the request ([Errno 110] Connection timed out)",
        "2.4 GHz: the keyboard answers, battery 81%",
    ]
