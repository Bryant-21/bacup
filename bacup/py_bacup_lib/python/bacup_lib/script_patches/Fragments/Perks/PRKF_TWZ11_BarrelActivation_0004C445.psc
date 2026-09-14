Function Fragment_Entry_00(ObjectReference akTargetRef, Actor akActor)
	If akActor == Game.GetPlayer() && TWZ11 != None
		TWZ11.SetStage(Barrel01PickupStage)
		TWZ11.SetStage(Barrel01DisposedStage)
	EndIf
EndFunction

Function Fragment_Entry_01(ObjectReference akTargetRef, Actor akActor)
	If akActor == Game.GetPlayer() && TWZ11 != None
		TWZ11.SetStage(Barrel02PickupStage)
		TWZ11.SetStage(Barrel02DisposedStage)
	EndIf
EndFunction

Function Fragment_Entry_02(ObjectReference akTargetRef, Actor akActor)
	If akActor == Game.GetPlayer() && TWZ11 != None
		TWZ11.SetStage(Barrel03PickupStage)
		TWZ11.SetStage(Barrel03DisposedStage)
	EndIf
EndFunction
