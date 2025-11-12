from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset







T = TypeVar("T", bound="DeployPerpForBeaconResponse")



@_attrs_define
class DeployPerpForBeaconResponse:
    """ Response from deploying a perpetual contract

        Attributes:
            perp_id (str): 32-byte perpetual pool identifier (hex string with 0x prefix)
            perp_manager_address (str): Address of the PerpManager contract
            transaction_hash (str): Transaction hash
     """

    perp_id: str
    perp_manager_address: str
    transaction_hash: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        perp_id = self.perp_id

        perp_manager_address = self.perp_manager_address

        transaction_hash = self.transaction_hash


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "perp_id": perp_id,
            "perp_manager_address": perp_manager_address,
            "transaction_hash": transaction_hash,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        perp_id = d.pop("perp_id")

        perp_manager_address = d.pop("perp_manager_address")

        transaction_hash = d.pop("transaction_hash")

        deploy_perp_for_beacon_response = cls(
            perp_id=perp_id,
            perp_manager_address=perp_manager_address,
            transaction_hash=transaction_hash,
        )


        deploy_perp_for_beacon_response.additional_properties = d
        return deploy_perp_for_beacon_response

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
