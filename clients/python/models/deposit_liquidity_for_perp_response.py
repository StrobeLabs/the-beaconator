from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset







T = TypeVar("T", bound="DepositLiquidityForPerpResponse")



@_attrs_define
class DepositLiquidityForPerpResponse:
    """ Response from depositing liquidity to a perpetual

        Attributes:
            maker_position_id (str): Maker position ID from MakerPositionOpened event
            approval_transaction_hash (str): USDC approval transaction hash
            deposit_transaction_hash (str): Liquidity deposit transaction hash
     """

    maker_position_id: str
    approval_transaction_hash: str
    deposit_transaction_hash: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        maker_position_id = self.maker_position_id

        approval_transaction_hash = self.approval_transaction_hash

        deposit_transaction_hash = self.deposit_transaction_hash


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "maker_position_id": maker_position_id,
            "approval_transaction_hash": approval_transaction_hash,
            "deposit_transaction_hash": deposit_transaction_hash,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        maker_position_id = d.pop("maker_position_id")

        approval_transaction_hash = d.pop("approval_transaction_hash")

        deposit_transaction_hash = d.pop("deposit_transaction_hash")

        deposit_liquidity_for_perp_response = cls(
            maker_position_id=maker_position_id,
            approval_transaction_hash=approval_transaction_hash,
            deposit_transaction_hash=deposit_transaction_hash,
        )


        deposit_liquidity_for_perp_response.additional_properties = d
        return deposit_liquidity_for_perp_response

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
