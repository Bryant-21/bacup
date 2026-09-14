Function Fragment_Stage_0051_Item_00()
	TWZ11_Script controller = (Self as Quest) as TWZ11_Script
	If controller != None
		controller.CheckForAllDumped()
	EndIf
EndFunction

Function Fragment_Stage_0052_Item_00()
	TWZ11_Script controller = (Self as Quest) as TWZ11_Script
	If controller != None
		controller.CheckForAllDumped()
	EndIf
EndFunction

Function Fragment_Stage_0053_Item_00()
	TWZ11_Script controller = (Self as Quest) as TWZ11_Script
	If controller != None
		controller.CheckForAllDumped()
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(100, True)
	SetObjectiveDisplayed(101, True)
	SetObjectiveDisplayed(102, True)

	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None
		If TWZ11_BarrelActivation != None && !playerRef.HasPerk(TWZ11_BarrelActivation)
			playerRef.AddPerk(TWZ11_BarrelActivation)
		EndIf
		If Agility != None && playerRef.GetValue(Agility) >= 5.0
			SetObjectiveDisplayed(200, True)
		EndIf
	EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None && pTWz11EBSTracker != None
		playerRef.SetValue(pTWz11EBSTracker, 1.0)
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(100, True)
	TWZ11_Script controller = (Self as Quest) as TWZ11_Script
	If controller != None
		controller.CheckForAllBarrels()
	EndIf
EndFunction

Function Fragment_Stage_0201_Item_00()
	SetObjectiveCompleted(101, True)
	TWZ11_Script controller = (Self as Quest) as TWZ11_Script
	If controller != None
		controller.CheckForAllBarrels()
	EndIf
EndFunction

Function Fragment_Stage_0202_Item_00()
	SetObjectiveCompleted(102, True)
	TWZ11_Script controller = (Self as Quest) as TWZ11_Script
	If controller != None
		controller.CheckForAllBarrels()
	EndIf
EndFunction

Function Fragment_Stage_0220_Item_00()
	ObjectReference barrelRef = Alias_Barrel01.GetReference()
	If barrelRef != None
		If QSTTWZ11BarrelSink != None
			QSTTWZ11BarrelSink.Play(barrelRef)
		EndIf
		barrelRef.Disable()
	EndIf
	SetStage(200)
EndFunction

Function Fragment_Stage_0221_Item_00()
	ObjectReference barrelRef = Alias_Barrel02.GetReference()
	If barrelRef != None
		If QSTTWZ11BarrelSink != None
			QSTTWZ11BarrelSink.Play(barrelRef)
		EndIf
		barrelRef.Disable()
	EndIf
	SetStage(201)
EndFunction

Function Fragment_Stage_0222_Item_00()
	ObjectReference barrelRef = Alias_Barrel03.GetReference()
	If barrelRef != None
		If QSTTWZ11BarrelSink != None
			QSTTWZ11BarrelSink.Play(barrelRef)
		EndIf
		barrelRef.Disable()
	EndIf
	SetStage(202)
EndFunction

Function Fragment_Stage_0300_Item_00()
	If IsObjectiveDisplayed(200)
		SetObjectiveDisplayed(200, False)
	EndIf
	SetObjectiveDisplayed(300, True)
EndFunction

Function Fragment_Stage_1000_Item_00()
	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None
		If TWZ11_ToxicBarrel01 != None && playerRef.GetItemCount(TWZ11_ToxicBarrel01) > 0
			playerRef.RemoveItem(TWZ11_ToxicBarrel01, playerRef.GetItemCount(TWZ11_ToxicBarrel01), True)
		EndIf
		If TWZ11_ToxicBarrel02 != None && playerRef.GetItemCount(TWZ11_ToxicBarrel02) > 0
			playerRef.RemoveItem(TWZ11_ToxicBarrel02, playerRef.GetItemCount(TWZ11_ToxicBarrel02), True)
		EndIf
		If TWZ11_ToxicBarrel03 != None && playerRef.GetItemCount(TWZ11_ToxicBarrel03) > 0
			playerRef.RemoveItem(TWZ11_ToxicBarrel03, playerRef.GetItemCount(TWZ11_ToxicBarrel03), True)
		EndIf
		If TWZ11_BarrelActivation != None && playerRef.HasPerk(TWZ11_BarrelActivation)
			playerRef.RemovePerk(TWZ11_BarrelActivation)
		EndIf
	EndIf

	ObjectReference dumpRef = Alias_Dumpster.GetReference()
	If dumpRef != None && QSTTWZ11BarrelDump != None
		QSTTWZ11BarrelDump.Play(dumpRef)
	EndIf
	CompleteAllObjectives()
	Stop()
EndFunction
