"""Global state management for Behavior Graph Editor.

Manages global graph-level data:
- Global variables (named float/int values)
- Events (named event identifiers)
- Transitions (state machine transition definitions)
- Payloads (string event payloads)
- Properties (custom property definitions)
"""

from typing import List, Dict, Any, Optional
from dataclasses import dataclass, field


@dataclass
class GlobalVariable:
    """Represents a global variable in the behavior graph."""

    name: str = ""
    value: float = 0.0
    index: int = 0

    def to_dict(self) -> Dict[str, Any]:
        return {
            "name": self.name,
            "value": self.value,
            "index": self.index,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "GlobalVariable":
        return cls(
            name=data.get("name", ""),
            value=data.get("value", 0.0),
            index=data.get("index", 0),
        )


@dataclass
class GlobalEvent:
    """Represents a global event in the behavior graph."""

    name: str = ""
    event_id: int = -1

    def to_dict(self) -> Dict[str, Any]:
        return {
            "name": self.name,
            "eventID": self.event_id,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "GlobalEvent":
        return cls(
            name=data.get("name", ""),
            event_id=data.get("eventID", -1),
        )


@dataclass
class GlobalPayload:
    """Represents a global payload (string) for events."""

    name: str = ""

    def to_dict(self) -> Dict[str, Any]:
        return {
            "name": self.name,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "GlobalPayload":
        return cls(
            name=data.get("name", ""),
        )


@dataclass
class GlobalTransition:
    """Represents a state machine transition definition."""

    trigger_interval_enter_event_id: int = -1
    trigger_interval_exit_event_id: int = -1
    trigger_interval_enter_time: float = 0.0
    trigger_interval_exit_time: float = 0.0
    initiate_interval_enter_event_id: int = -1
    initiate_interval_exit_event_id: int = -1
    initiate_interval_enter_time: float = 0.0
    initiate_interval_exit_time: float = 0.0
    transition_index: int = 0
    condition: Optional[str] = None
    event_id: int = -1
    to_state_id: int = -1
    from_nested_state_id: int = -1
    to_nested_state_id: int = -1
    priority: int = 0
    flags: List[bool] = field(default_factory=lambda: [False] * 15)

    def to_dict(self) -> Dict[str, Any]:
        return {
            "triggerInterval": {
                "enterEventId": self.trigger_interval_enter_event_id,
                "exitEventId": self.trigger_interval_exit_event_id,
                "enterTime": self.trigger_interval_enter_time,
                "exitTime": self.trigger_interval_exit_time,
            },
            "initiateInterval": {
                "enterEventId": self.initiate_interval_enter_event_id,
                "exitEventId": self.initiate_interval_exit_event_id,
                "enterTime": self.initiate_interval_enter_time,
                "exitTime": self.initiate_interval_exit_time,
            },
            "transitionIndex": self.transition_index,
            "condition": self.condition,
            "eventId": self.event_id,
            "toStateId": self.to_state_id,
            "fromNestedStateId": self.from_nested_state_id,
            "toNestedStateId": self.to_nested_state_id,
            "priority": self.priority,
            "flags": self.flags,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "GlobalTransition":
        ti = data.get("triggerInterval", {})
        ii = data.get("initiateInterval", {})
        return cls(
            trigger_interval_enter_event_id=ti.get("enterEventId", -1),
            trigger_interval_exit_event_id=ti.get("exitEventId", -1),
            trigger_interval_enter_time=ti.get("enterTime", 0.0),
            trigger_interval_exit_time=ti.get("exitTime", 0.0),
            initiate_interval_enter_event_id=ii.get("enterEventId", -1),
            initiate_interval_exit_event_id=ii.get("exitEventId", -1),
            initiate_interval_enter_time=ii.get("enterTime", 0.0),
            initiate_interval_exit_time=ii.get("exitTime", 0.0),
            transition_index=data.get("transitionIndex", 0),
            condition=data.get("condition"),
            event_id=data.get("eventId", -1),
            to_state_id=data.get("toStateId", -1),
            from_nested_state_id=data.get("fromNestedStateId", -1),
            to_nested_state_id=data.get("toNestedStateId", -1),
            priority=data.get("priority", 0),
            flags=data.get("flags", [False] * 15),
        )


@dataclass
class GlobalProperty:
    """Represents a custom property definition."""

    name: str = ""
    property_type: str = "float"
    value: Any = 0.0

    def to_dict(self) -> Dict[str, Any]:
        return {
            "name": self.name,
            "type": self.property_type,
            "value": self.value,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "GlobalProperty":
        return cls(
            name=data.get("name", ""),
            property_type=data.get("type", "float"),
            value=data.get("value", 0.0),
        )


class GlobalState:
    """Manages all global state for the behavior graph."""

    def __init__(self):
        self.variables: List[GlobalVariable] = []
        self.events: List[GlobalEvent] = []
        self.payloads: List[GlobalPayload] = []
        self.transitions: List[GlobalTransition] = []
        self.properties: List[GlobalProperty] = []

    def add_variable(self, name: str, value: float = 0.0) -> GlobalVariable:
        var = GlobalVariable(name=name, value=value, index=len(self.variables))
        self.variables.append(var)
        return var

    def add_event(self, name: str, event_id: int = -1) -> GlobalEvent:
        event = GlobalEvent(name=name, event_id=event_id)
        self.events.append(event)
        return event

    def add_payload(self, name: str) -> GlobalPayload:
        payload = GlobalPayload(name=name)
        self.payloads.append(payload)
        return payload

    def add_transition(self) -> GlobalTransition:
        trans = GlobalTransition()
        self.transitions.append(trans)
        return trans

    def add_property(
        self, name: str, prop_type: str = "float", value: Any = 0.0
    ) -> GlobalProperty:
        prop = GlobalProperty(name=name, property_type=prop_type, value=value)
        self.properties.append(prop)
        return prop

    def clear(self):
        self.variables.clear()
        self.events.clear()
        self.payloads.clear()
        self.transitions.clear()
        self.properties.clear()

    def to_dict(self) -> Dict[str, Any]:
        """Handles both raw dicts (from XML import/dialogs) and dataclass instances."""
        def _item_to_dict(item):
            if isinstance(item, dict):
                return item
            return item.to_dict()

        return {
            "variables": [_item_to_dict(v) for v in self.variables],
            "events": [_item_to_dict(e) for e in self.events],
            "payloads": [_item_to_dict(p) for p in self.payloads],
            "transitions": [_item_to_dict(t) for t in self.transitions],
            "properties": [_item_to_dict(p) for p in self.properties],
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "GlobalState":
        """Stores items as raw dicts for compatibility with XML import and dialogs."""
        state = cls()
        state.variables = list(data.get("variables", []))
        state.events = list(data.get("events", []))
        state.payloads = list(data.get("payloads", []))
        state.transitions = list(data.get("transitions", []))
        state.properties = list(data.get("properties", []))
        return state
