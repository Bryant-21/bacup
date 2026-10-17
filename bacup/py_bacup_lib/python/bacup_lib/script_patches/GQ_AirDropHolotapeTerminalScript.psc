Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
	If auiMenuItemID != 1 || akTerminalRef == None || RadioStationRefType == None || !akTerminalRef.HasRefType(RadioStationRefType)
		Return
	EndIf

	Actor playerRef = Game.GetPlayer()
	GQ_DropGovtScript governmentDrop = Game.GetFormFromFile(0x0000FFE7, "SeventySix.esm") as GQ_DropGovtScript
	If playerRef == None || GQ_DropGovt01Holotape == None || playerRef.GetItemCount(GQ_DropGovt01Holotape) == 0 || governmentDrop == None
		If GQ_DropGovtAlreadyInProgress != None
			GQ_DropGovtAlreadyInProgress.Show()
		EndIf
		Return
	EndIf

	; GQ_DropGovtMaster is single-instance here; the holotape is spent only when a drop starts.
	If !governmentDrop.RequestLocalAirDrop(akTerminalRef.GetCurrentLocation())
		If GQ_DropGovtAlreadyInProgress != None
			GQ_DropGovtAlreadyInProgress.Show()
		EndIf
		Return
	EndIf
	playerRef.RemoveItem(GQ_DropGovt01Holotape, 1, True)
	If GQ_DropGovtAirDropSuccess != None
		GQ_DropGovtAirDropSuccess.Show()
	EndIf
EndEvent
