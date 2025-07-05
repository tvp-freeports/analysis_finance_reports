class Range:
    def __init__(self, start, end):
        self._start = start
        self._end = end

    @property
    def start(self):
        return self._start

    @property
    def end(self):
        return self._end

    @property
    def size(self):
        return self.end - self.start

    def __contains__(self, value):
        return (self.start is None or self.start <= value) and (
            self.end is None or value <= self.end
        )

    def __str__(self):
        return f"[{self.start},{self.end}]"
