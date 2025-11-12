from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset







T = TypeVar("T", bound="DeployPerpForBeaconRequest")



@_attrs_define
class DeployPerpForBeaconRequest:
    """ Deploy a perpetual contract for a specific beacon

        Attributes:
            beacon_address (str): Ethereum address of the beacon contract
            fees_module (str): Address of the fees configuration module
            margin_ratios_module (str): Address of the margin ratios configuration module
            lockup_period_module (str): Address of the lockup period configuration module
            sqrt_price_impact_limit_module (str): Address of the sqrt price impact limit configuration module
            starting_sqrt_price_x96 (str): Starting sqrt price in Q96 format as string
     """

    beacon_address: str
    fees_module: str
    margin_ratios_module: str
    lockup_period_module: str
    sqrt_price_impact_limit_module: str
    starting_sqrt_price_x96: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        beacon_address = self.beacon_address

        fees_module = self.fees_module

        margin_ratios_module = self.margin_ratios_module

        lockup_period_module = self.lockup_period_module

        sqrt_price_impact_limit_module = self.sqrt_price_impact_limit_module

        starting_sqrt_price_x96 = self.starting_sqrt_price_x96


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "beacon_address": beacon_address,
            "fees_module": fees_module,
            "margin_ratios_module": margin_ratios_module,
            "lockup_period_module": lockup_period_module,
            "sqrt_price_impact_limit_module": sqrt_price_impact_limit_module,
            "starting_sqrt_price_x96": starting_sqrt_price_x96,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        beacon_address = d.pop("beacon_address")

        fees_module = d.pop("fees_module")

        margin_ratios_module = d.pop("margin_ratios_module")

        lockup_period_module = d.pop("lockup_period_module")

        sqrt_price_impact_limit_module = d.pop("sqrt_price_impact_limit_module")

        starting_sqrt_price_x96 = d.pop("starting_sqrt_price_x96")

        deploy_perp_for_beacon_request = cls(
            beacon_address=beacon_address,
            fees_module=fees_module,
            margin_ratios_module=margin_ratios_module,
            lockup_period_module=lockup_period_module,
            sqrt_price_impact_limit_module=sqrt_price_impact_limit_module,
            starting_sqrt_price_x96=starting_sqrt_price_x96,
        )


        deploy_perp_for_beacon_request.additional_properties = d
        return deploy_perp_for_beacon_request

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
