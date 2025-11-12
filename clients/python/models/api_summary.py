from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast

if TYPE_CHECKING:
  from ..models.endpoint_info import EndpointInfo





T = TypeVar("T", bound="ApiSummary")



@_attrs_define
class ApiSummary:
    """ 
        Attributes:
            total_endpoints (int):
            working_endpoints (int):
            not_implemented (int):
            deprecated (int):
            endpoints (list[EndpointInfo]):
     """

    total_endpoints: int
    working_endpoints: int
    not_implemented: int
    deprecated: int
    endpoints: list[EndpointInfo]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.endpoint_info import EndpointInfo
        total_endpoints = self.total_endpoints

        working_endpoints = self.working_endpoints

        not_implemented = self.not_implemented

        deprecated = self.deprecated

        endpoints = []
        for endpoints_item_data in self.endpoints:
            endpoints_item = endpoints_item_data.to_dict()
            endpoints.append(endpoints_item)




        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "total_endpoints": total_endpoints,
            "working_endpoints": working_endpoints,
            "not_implemented": not_implemented,
            "deprecated": deprecated,
            "endpoints": endpoints,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.endpoint_info import EndpointInfo
        d = dict(src_dict)
        total_endpoints = d.pop("total_endpoints")

        working_endpoints = d.pop("working_endpoints")

        not_implemented = d.pop("not_implemented")

        deprecated = d.pop("deprecated")

        endpoints = []
        _endpoints = d.pop("endpoints")
        for endpoints_item_data in (_endpoints):
            endpoints_item = EndpointInfo.from_dict(endpoints_item_data)



            endpoints.append(endpoints_item)


        api_summary = cls(
            total_endpoints=total_endpoints,
            working_endpoints=working_endpoints,
            not_implemented=not_implemented,
            deprecated=deprecated,
            endpoints=endpoints,
        )


        api_summary.additional_properties = d
        return api_summary

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
