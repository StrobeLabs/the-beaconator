from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset







T = TypeVar("T", bound="RegisterBeaconRequest")



@_attrs_define
class RegisterBeaconRequest:
    """ Register an existing beacon with the registry

        Attributes:
            beacon_address (str): Ethereum address of the beacon contract
            registry_address (str): Ethereum address of the beacon registry contract
     """

    beacon_address: str
    registry_address: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        beacon_address = self.beacon_address

        registry_address = self.registry_address


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "beacon_address": beacon_address,
            "registry_address": registry_address,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        beacon_address = d.pop("beacon_address")

        registry_address = d.pop("registry_address")

        register_beacon_request = cls(
            beacon_address=beacon_address,
            registry_address=registry_address,
        )


        register_beacon_request.additional_properties = d
        return register_beacon_request

    @property
    def additional_keys(self) -> list[str]:
        return list(self.additional_properties.keys())

    def __getitem__(self, key: str) -> Any:
        return self.additional_properties[key]

    def __setitem__(self, key: str, value: Any) -> None:
        self.additional_properties[key] = value

    def __delitem__(self, key: str) -> None:
        del self.additional_properties[key]

    def __contains__(self, key: str) -> bool:
        return key in self.additional_properties
