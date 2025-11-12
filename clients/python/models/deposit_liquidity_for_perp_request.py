from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast






T = TypeVar("T", bound="DepositLiquidityForPerpRequest")



@_attrs_define
class DepositLiquidityForPerpRequest:
    """ Deposit liquidity for a perpetual contract

        Attributes:
            perp_id (str): Perpetual pool ID as hex string (with or without 0x prefix)
            margin_amount_usdc (str): USDC margin amount in 6 decimals (e.g., "50000000" for 50 USDC)

                Margin constraints are enforced by on-chain modules. The margin ratios module defines minimum and maximum
                allowed margins based on market configuration.

                Current liquidity scaling: margin × 500,000 = final liquidity amount
            holder (None | str | Unset): Optional holder address (defaults to wallet address if not provided)
            max_amt0_in (None | str | Unset): Maximum amount of token0 to deposit (slippage protection), optional
            max_amt1_in (None | str | Unset): Maximum amount of token1 to deposit (slippage protection), optional
            tick_spacing (int | None | Unset): Tick spacing for the liquidity position (defaults to 30)
            tick_lower (int | None | Unset): Lower tick bound for the liquidity position (defaults to 24390)
            tick_upper (int | None | Unset): Upper tick bound for the liquidity position (defaults to 53850)
     """

    perp_id: str
    margin_amount_usdc: str
    holder: None | str | Unset = UNSET
    max_amt0_in: None | str | Unset = UNSET
    max_amt1_in: None | str | Unset = UNSET
    tick_spacing: int | None | Unset = UNSET
    tick_lower: int | None | Unset = UNSET
    tick_upper: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        perp_id = self.perp_id

        margin_amount_usdc = self.margin_amount_usdc

        holder: None | str | Unset
        if isinstance(self.holder, Unset):
            holder = UNSET
        else:
            holder = self.holder

        max_amt0_in: None | str | Unset
        if isinstance(self.max_amt0_in, Unset):
            max_amt0_in = UNSET
        else:
            max_amt0_in = self.max_amt0_in

        max_amt1_in: None | str | Unset
        if isinstance(self.max_amt1_in, Unset):
            max_amt1_in = UNSET
        else:
            max_amt1_in = self.max_amt1_in

        tick_spacing: int | None | Unset
        if isinstance(self.tick_spacing, Unset):
            tick_spacing = UNSET
        else:
            tick_spacing = self.tick_spacing

        tick_lower: int | None | Unset
        if isinstance(self.tick_lower, Unset):
            tick_lower = UNSET
        else:
            tick_lower = self.tick_lower

        tick_upper: int | None | Unset
        if isinstance(self.tick_upper, Unset):
            tick_upper = UNSET
        else:
            tick_upper = self.tick_upper


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "perp_id": perp_id,
            "margin_amount_usdc": margin_amount_usdc,
        })
        if holder is not UNSET:
            field_dict["holder"] = holder
        if max_amt0_in is not UNSET:
            field_dict["max_amt0_in"] = max_amt0_in
        if max_amt1_in is not UNSET:
            field_dict["max_amt1_in"] = max_amt1_in
        if tick_spacing is not UNSET:
            field_dict["tick_spacing"] = tick_spacing
        if tick_lower is not UNSET:
            field_dict["tick_lower"] = tick_lower
        if tick_upper is not UNSET:
            field_dict["tick_upper"] = tick_upper

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        perp_id = d.pop("perp_id")

        margin_amount_usdc = d.pop("margin_amount_usdc")

        def _parse_holder(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        holder = _parse_holder(d.pop("holder", UNSET))


        def _parse_max_amt0_in(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        max_amt0_in = _parse_max_amt0_in(d.pop("max_amt0_in", UNSET))


        def _parse_max_amt1_in(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        max_amt1_in = _parse_max_amt1_in(d.pop("max_amt1_in", UNSET))


        def _parse_tick_spacing(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        tick_spacing = _parse_tick_spacing(d.pop("tick_spacing", UNSET))


        def _parse_tick_lower(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        tick_lower = _parse_tick_lower(d.pop("tick_lower", UNSET))


        def _parse_tick_upper(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        tick_upper = _parse_tick_upper(d.pop("tick_upper", UNSET))


        deposit_liquidity_for_perp_request = cls(
            perp_id=perp_id,
            margin_amount_usdc=margin_amount_usdc,
            holder=holder,
            max_amt0_in=max_amt0_in,
            max_amt1_in=max_amt1_in,
            tick_spacing=tick_spacing,
            tick_lower=tick_lower,
            tick_upper=tick_upper,
        )


        deposit_liquidity_for_perp_request.additional_properties = d
        return deposit_liquidity_for_perp_request

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
