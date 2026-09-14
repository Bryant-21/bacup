Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
	Actor playerRef = Game.GetPlayer()
	Holotape dataHolotape = None

	If auiMenuItemID == 1
		dataHolotape = RS03_BalanceWaterDataHolotape
	ElseIf auiMenuItemID == 2
		dataHolotape = RS03_BalanceSoilDataHolotape
	ElseIf auiMenuItemID == 3
		dataHolotape = RS03_BalanceAirDataHolotape
	EndIf

	If dataHolotape != None
		If playerRef.GetItemCount(dataHolotape) > 0
			playerRef.RemoveItem(dataHolotape, 1, True)
		Else
			RS03_Balance_MissingDataMessage.Show()
		EndIf
	EndIf
EndEvent
