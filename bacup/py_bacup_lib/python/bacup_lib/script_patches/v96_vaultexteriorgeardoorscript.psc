Event OnLoad()
	; FO76 SendRMIToServer(...) -> direct local call.
	Self.RequestUpdateExteriorGearDoorStateForPlayerOnClientLoad(Game.GetPlayer())
EndEvent
