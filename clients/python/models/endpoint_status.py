from enum import Enum

class EndpointStatus(str, Enum):
    DEPRECATED = "Deprecated"
    NOTIMPLEMENTED = "NotImplemented"
    WORKING = "Working"

    def __str__(self) -> str:
        return str(self.value)
