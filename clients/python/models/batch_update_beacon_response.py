from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast

if TYPE_CHECKING:
  from ..models.beacon_update_result import BeaconUpdateResult





T = TypeVar("T", bound="BatchUpdateBeaconResponse")



@_attrs_define
class BatchUpdateBeaconResponse:
    """ Response from batch beacon update operation

        Attributes:
            results (list[BeaconUpdateResult]): Individual results for each beacon
            total_requested (int): Total number of updates requested
            successful_updates (int): Number of successful updates
            failed_updates (int): Number of failed updates
     """

    results: list[BeaconUpdateResult]
    total_requested: int
    successful_updates: int
    failed_updates: int
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.beacon_update_result import BeaconUpdateResult
        results = []
        for results_item_data in self.results:
            results_item = results_item_data.to_dict()
            results.append(results_item)



        total_requested = self.total_requested

        successful_updates = self.successful_updates

        failed_updates = self.failed_updates


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "results": results,
            "total_requested": total_requested,
            "successful_updates": successful_updates,
            "failed_updates": failed_updates,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.beacon_update_result import BeaconUpdateResult
        d = dict(src_dict)
        results = []
        _results = d.pop("results")
        for results_item_data in (_results):
            results_item = BeaconUpdateResult.from_dict(results_item_data)



            results.append(results_item)


        total_requested = d.pop("total_requested")

        successful_updates = d.pop("successful_updates")

        failed_updates = d.pop("failed_updates")

        batch_update_beacon_response = cls(
            results=results,
            total_requested=total_requested,
            successful_updates=successful_updates,
            failed_updates=failed_updates,
        )


        batch_update_beacon_response.additional_properties = d
        return batch_update_beacon_response

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
