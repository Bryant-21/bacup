Function Fragment_Entry_00(ObjectReference akTargetRef, Actor akActor)
	If akActor == Game.GetPlayer() && TWZ09 != None
		TWZ09.SetStage(10)
	EndIf
EndFunction
