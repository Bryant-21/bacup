Event OnBegin(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
	If !ExchangeOnEnd
		Exchange(akSpeakerRef)
	EndIf
EndEvent

Event OnEnd(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
	If ExchangeOnEnd
		Exchange(akSpeakerRef)
	EndIf
EndEvent

Function Exchange(ObjectReference akSpeakerRef)
	Actor playerRef = Game.GetPlayer()
	If playerRef == None
		Return
	EndIf
	Bool playerPays = PlayerCurrency_Type != None && PlayerCurrency_Amount > 0
	If playerPays && playerRef.GetItemCount(PlayerCurrency_Type) < PlayerCurrency_Amount
		Return
	EndIf
	If PlayerItem && playerRef.GetItemCount(PlayerItem) < PlayerItem_AmountToGive
		Return
	EndIf
	If PlayerItem
		playerRef.RemoveItem(PlayerItem, PlayerItem_AmountToGive, PlayerItem_GiveSilently, akSpeakerRef)
	EndIf
	If playerPays
		playerRef.RemoveItem(PlayerCurrency_Type, PlayerCurrency_Amount, False, akSpeakerRef)
	EndIf
	If NPCItem
		playerRef.AddItem(NPCItem, NPCItem_AmountToGive, NPCItem_GiveSilently)
	EndIf
	If NPCCurrency_Type != None && NPCCurrency_Amount > 0
		playerRef.AddItem(NPCCurrency_Type, NPCCurrency_Amount, False)
	EndIf
EndFunction
