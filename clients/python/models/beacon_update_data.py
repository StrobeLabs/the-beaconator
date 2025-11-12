from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset







T = TypeVar("T", bound="BeaconUpdateData")



@_attrs_define
class BeaconUpdateData:
    """ Beacon update data for batch operations

        Attributes:
            beacon_address (str): Ethereum address of the beacon contract (with or without 0x prefix)
            proof (str): Zero-knowledge proof data as hex string (with 0x prefix)
            public_signals (str): Public signals from the proof as hex string (with 0x prefix)
     """

    beacon_address: str
    proof: str
    public_signals: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        beacon_address = self.beacon_address

        proof = self.proof

        public_signals = self.public_signals


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "beacon_address": beacon_address,
            "proof": proof,
            "public_signals": public_signals,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        beacon_address = d.pop("beacon_address")

        proof = d.pop("proof")

        public_signals = d.pop("public_signals")

        beacon_update_data = cls(
            beacon_address=beacon_address,
            proof=proof,
            public_signals=public_signals,
        )


        beacon_update_data.additional_properties = d
        return beacon_update_data

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
