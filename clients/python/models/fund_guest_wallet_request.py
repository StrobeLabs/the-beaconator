from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset







T = TypeVar("T", bound="FundGuestWalletRequest")



@_attrs_define
class FundGuestWalletRequest:
    """ Fund a guest wallet with USDC and ETH

        Attributes:
            wallet_address (str): Ethereum address of the wallet to fund
            usdc_amount (str): USDC amount in 6 decimals (e.g., "100000000" for 100 USDC)
            eth_amount (str): ETH amount in wei (e.g., "1000000000000000" for 0.001 ETH)
     """

    wallet_address: str
    usdc_amount: str
    eth_amount: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        wallet_address = self.wallet_address

        usdc_amount = self.usdc_amount

        eth_amount = self.eth_amount


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "wallet_address": wallet_address,
            "usdc_amount": usdc_amount,
            "eth_amount": eth_amount,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        wallet_address = d.pop("wallet_address")

        usdc_amount = d.pop("usdc_amount")

        eth_amount = d.pop("eth_amount")

        fund_guest_wallet_request = cls(
            wallet_address=wallet_address,
            usdc_amount=usdc_amount,
            eth_amount=eth_amount,
        )


        fund_guest_wallet_request.additional_properties = d
        return fund_guest_wallet_request

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
