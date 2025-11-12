from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast






T = TypeVar("T", bound="BatchCreatePerpcityBeaconResponse")



@_attrs_define
class BatchCreatePerpcityBeaconResponse:
    """ Response from batch Perpcity beacon creation

        Attributes:
            created_count (int): Number of successfully created beacons
            beacon_addresses (list[str]): List of beacon addresses (hex strings with 0x prefix)
            failed_count (int): Number of failed creations
            errors (list[str]): Error messages for failed creations
     """

    created_count: int
    beacon_addresses: list[str]
    failed_count: int
    errors: list[str]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        created_count = self.created_count

        beacon_addresses = self.beacon_addresses



        failed_count = self.failed_count

        errors = self.errors




        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "created_count": created_count,
            "beacon_addresses": beacon_addresses,
            "failed_count": failed_count,
            "errors": errors,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        created_count = d.pop("created_count")

        beacon_addresses = cast(list[str], d.pop("beacon_addresses"))


        failed_count = d.pop("failed_count")

        errors = cast(list[str], d.pop("errors"))


        batch_create_perpcity_beacon_response = cls(
            created_count=created_count,
            beacon_addresses=beacon_addresses,
            failed_count=failed_count,
            errors=errors,
        )


        batch_create_perpcity_beacon_response.additional_properties = d
        return batch_create_perpcity_beacon_response

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
