from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset







T = TypeVar("T", bound="CreateVerifiableBeaconRequest")



@_attrs_define
class CreateVerifiableBeaconRequest:
    """ Create a verifiable beacon with zero-knowledge proof support and TWAP

        Attributes:
            verifier_address (str): Halo2 verifier contract address
            initial_data (str): Initial data value (MUST be pre-scaled by 2^96 if representing a decimal)
            initial_cardinality (int): Initial TWAP observation slots (typically 100-1000)
     """

    verifier_address: str
    initial_data: str
    initial_cardinality: int
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        verifier_address = self.verifier_address

        initial_data = self.initial_data

        initial_cardinality = self.initial_cardinality


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "verifier_address": verifier_address,
            "initial_data": initial_data,
            "initial_cardinality": initial_cardinality,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        verifier_address = d.pop("verifier_address")

        initial_data = d.pop("initial_data")

        initial_cardinality = d.pop("initial_cardinality")

        create_verifiable_beacon_request = cls(
            verifier_address=verifier_address,
            initial_data=initial_data,
            initial_cardinality=initial_cardinality,
        )


        create_verifiable_beacon_request.additional_properties = d
        return create_verifiable_beacon_request

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
