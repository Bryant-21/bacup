Event OnCellLoad()
	; FO76 SendRMIToServer("CheckMODUSRevealStateRMI") -> direct local call.
	Self.CheckMODUSRevealStateRMI(Game.GetPlayer())
EndEvent
