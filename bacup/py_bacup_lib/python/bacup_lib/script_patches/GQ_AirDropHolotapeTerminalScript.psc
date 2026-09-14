Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
	If auiMenuItemID != 1 || akTerminalRef == None || RadioStationRefType == None || !akTerminalRef.HasRefType(RadioStationRefType)
		Return
	EndIf

	Actor playerRef = Game.GetPlayer()
	If playerRef == None || GQ_DropGovt01Holotape == None || playerRef.GetItemCount(GQ_DropGovt01Holotape) == 0
		If GQ_DropGovtAlreadyInProgress != None
			GQ_DropGovtAlreadyInProgress.Show()
		EndIf
		Return
	EndIf

	playerRef.RemoveItem(GQ_DropGovt01Holotape, 1, True)
	If GQ_DropGovt01Keyword != None
		GQ_DropGovt01Keyword.SendStoryEvent(akLoc = akTerminalRef.GetCurrentLocation(), akRef1 = playerRef, akRef2 = akTerminalRef)
	EndIf
	If GQ_DropGovtAirDropSuccess != None
		GQ_DropGovtAirDropSuccess.Show()
	EndIf
EndEvent
