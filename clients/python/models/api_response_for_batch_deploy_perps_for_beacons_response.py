from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast

if TYPE_CHECKING:
  from ..models.batch_deploy_perps_for_beacons_response import BatchDeployPerpsForBeaconsResponse





T = TypeVar("T", bound="ApiResponseForBatchDeployPerpsForBeaconsResponse")



@_attrs_define
class ApiResponseForBatchDeployPerpsForBeaconsResponse:
    """ Standard API response wrapper

        Attributes:
            success (bool): Whether the request succeeded
            message (str): Human-readable message about the result
            data (BatchDeployPerpsForBeaconsResponse | None | Unset): Response data (null if request failed)
     """

    success: bool
    message: str
    data: BatchDeployPerpsForBeaconsResponse | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.batch_deploy_perps_for_beacons_response import BatchDeployPerpsForBeaconsResponse
        success = self.success

        message = self.message

        data: dict[str, Any] | None | Unset
        if isinstance(self.data, Unset):
            data = UNSET
        elif isinstance(self.data, BatchDeployPerpsForBeaconsResponse):
            data = self.data.to_dict()
        else:
            data = self.data


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "success": success,
            "message": message,
        })
        if data is not UNSET:
            field_dict["data"] = data

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.batch_deploy_perps_for_beacons_response import BatchDeployPerpsForBeaconsResponse
        d = dict(src_dict)
        success = d.pop("success")

        message = d.pop("message")

        def _parse_data(data: object) -> BatchDeployPerpsForBeaconsResponse | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                data_type_1 = BatchDeployPerpsForBeaconsResponse.from_dict(data)



                return data_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(BatchDeployPerpsForBeaconsResponse | None | Unset, data)

        data = _parse_data(d.pop("data", UNSET))


        api_response_for_batch_deploy_perps_for_beacons_response = cls(
            success=success,
            message=message,
            data=data,
        )


        api_response_for_batch_deploy_perps_for_beacons_response.additional_properties = d
        return api_response_for_batch_deploy_perps_for_beacons_response

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
