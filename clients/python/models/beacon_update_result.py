from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast






T = TypeVar("T", bound="BeaconUpdateResult")



@_attrs_define
class BeaconUpdateResult:
    """ Result of updating a single beacon

        Attributes:
            beacon_address (str): Address of the beacon that was updated
            success (bool): Whether the update succeeded
            transaction_hash (None | str | Unset): Transaction hash (if successful)
            error (None | str | Unset): Error message (if failed)
     """

    beacon_address: str
    success: bool
    transaction_hash: None | str | Unset = UNSET
    error: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        beacon_address = self.beacon_address

        success = self.success

        transaction_hash: None | str | Unset
        if isinstance(self.transaction_hash, Unset):
            transaction_hash = UNSET
        else:
            transaction_hash = self.transaction_hash

        error: None | str | Unset
        if isinstance(self.error, Unset):
            error = UNSET
        else:
            error = self.error


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "beacon_address": beacon_address,
            "success": success,
        })
        if transaction_hash is not UNSET:
            field_dict["transaction_hash"] = transaction_hash
        if error is not UNSET:
            field_dict["error"] = error

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        beacon_address = d.pop("beacon_address")

        success = d.pop("success")

        def _parse_transaction_hash(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        transaction_hash = _parse_transaction_hash(d.pop("transaction_hash", UNSET))


        def _parse_error(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        error = _parse_error(d.pop("error", UNSET))


        beacon_update_result = cls(
            beacon_address=beacon_address,
            success=success,
            transaction_hash=transaction_hash,
            error=error,
        )


        beacon_update_result.additional_properties = d
        return beacon_update_result

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
