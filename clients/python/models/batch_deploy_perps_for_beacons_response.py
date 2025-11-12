from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast






T = TypeVar("T", bound="BatchDeployPerpsForBeaconsResponse")



@_attrs_define
class BatchDeployPerpsForBeaconsResponse:
    """ Response from batch perpetual deployment

        Attributes:
            deployed_count (int): Number of successfully deployed perpetuals
            perp_ids (list[str]): List of perpetual pool IDs (hex strings with 0x prefix)
            failed_count (int): Number of failed deployments
            errors (list[str]): Error messages for failed deployments
     """

    deployed_count: int
    perp_ids: list[str]
    failed_count: int
    errors: list[str]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        deployed_count = self.deployed_count

        perp_ids = self.perp_ids



        failed_count = self.failed_count

        errors = self.errors




        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "deployed_count": deployed_count,
            "perp_ids": perp_ids,
            "failed_count": failed_count,
            "errors": errors,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        deployed_count = d.pop("deployed_count")

        perp_ids = cast(list[str], d.pop("perp_ids"))


        failed_count = d.pop("failed_count")

        errors = cast(list[str], d.pop("errors"))


        batch_deploy_perps_for_beacons_response = cls(
            deployed_count=deployed_count,
            perp_ids=perp_ids,
            failed_count=failed_count,
            errors=errors,
        )


        batch_deploy_perps_for_beacons_response.additional_properties = d
        return batch_deploy_perps_for_beacons_response

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
