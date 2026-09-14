Function Fragment_Stage_0010_Item_00()
	HandleControllerStage(10)
EndFunction

Function Fragment_Stage_0015_Item_00()
	HandleControllerStage(15)
EndFunction

Function Fragment_Stage_0020_Item_00()
	HandleControllerStage(20)
EndFunction

Function Fragment_Stage_0040_Item_00()
	HandleControllerStage(40)
EndFunction

Function Fragment_Stage_0060_Item_00()
	HandleControllerStage(60)
EndFunction

Function Fragment_Stage_0065_Item_00()
	HandleControllerStage(65)
EndFunction

Function Fragment_Stage_0070_Item_00()
	HandleControllerStage(70)
EndFunction

Function Fragment_Stage_0080_Item_00()
	HandleControllerStage(80)
EndFunction

Function Fragment_Stage_0100_Item_00()
	HandleControllerStage(100)
	Actor playerRef = Alias_currentPlayer.GetActorReference()
	If playerRef && W05_Community_BB_Completed
		playerRef.SetValue(W05_Community_BB_Completed, 1.0)
	EndIf
	ObjectReference lockedDoorRef = LockedDoor
	ObjectReference porchDoorRef = Alias_PorchDoorAlias.GetReference()
	If lockedDoorRef
		lockedDoorRef.Lock(True)
	EndIf
	If porchDoorRef
		porchDoorRef.Lock(True)
	EndIf
	If Alias_CurRenterAlias
		Alias_CurRenterAlias.Clear()
	EndIf
	Stop()
EndFunction

Function HandleControllerStage(Int aiStage)
	W05_Community_BB_Quest_Script controller = (Self as Quest) as W05_Community_BB_Quest_Script
	If controller
		controller.HandleQuestStage(aiStage)
	EndIf
EndFunction
