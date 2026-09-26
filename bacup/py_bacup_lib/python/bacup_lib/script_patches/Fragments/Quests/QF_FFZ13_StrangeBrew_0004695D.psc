Function Fragment_Stage_0001_Item_00()
	Location startLocation = None
	Location teapotLocation = None

	If QuestStartedAt != None
		startLocation = QuestStartedAt.GetLocation()
	EndIf
	If Teapot != None
		teapotLocation = Teapot.GetLocation()
	EndIf

	If startLocation != None && teapotLocation != None && startLocation == teapotLocation
		SetStage(10)
	Else
		SetStage(5)
	EndIf
EndFunction

Function Fragment_Stage_0005_Item_00()
	SetObjectiveDisplayed(2, True)
EndFunction

Function Fragment_Stage_0007_Item_00()
	SetObjectiveCompleted(2, True)
	SetObjectiveDisplayed(8, True)
EndFunction

Function Fragment_Stage_0010_Item_00()
	SetObjectiveDisplayed(8, True)
EndFunction

Function Fragment_Stage_0015_Item_00()
	; Sweetwater's welcome scene has no playable converted dialogue, so it either
	; refuses to start or ends without advancing the quest. Bound the wait so a
	; scene that never reports "finished" cannot strand the quest at stage 15.
	If WelcomeScene != None
		WelcomeScene.Start()
		Int waited = 0
		While WelcomeScene.IsPlaying() && waited < 20
			Utility.Wait(0.5)
			waited += 1
		EndWhile
	EndIf

	If !IsStageDone(30)
		SetStage(30)
	EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
	SetObjectiveCompleted(8, True)
	SetObjectiveDisplayed(10, True)
	WatchQuestGiver()

	Actor playerRef = None
	If Alias_QuestPlayer != None
		playerRef = Alias_QuestPlayer.GetActorReference()
	EndIf
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None && Honey != None && playerRef.GetItemCount(Honey) >= 10
		SetStage(50)
	EndIf
EndFunction

Function Fragment_Stage_0041_Item_00()
	ffz13_questscript questScript = (Self as Quest) as ffz13_questscript
	If questScript != None
		questScript.DisableHiveMarker(0)
	EndIf
EndFunction

Function Fragment_Stage_0042_Item_00()
	ffz13_questscript questScript = (Self as Quest) as ffz13_questscript
	If questScript != None
		questScript.DisableHiveMarker(1)
	EndIf
EndFunction

Function Fragment_Stage_0043_Item_00()
	ffz13_questscript questScript = (Self as Quest) as ffz13_questscript
	If questScript != None
		questScript.DisableHiveMarker(2)
	EndIf
EndFunction

Function Fragment_Stage_0044_Item_00()
	ffz13_questscript questScript = (Self as Quest) as ffz13_questscript
	If questScript != None
		questScript.DisableHiveMarker(3)
	EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
	SetObjectiveCompleted(10, True)
	SetObjectiveDisplayed(20, True)
	WatchQuestGiver()

	ObjectReference giverRef = None
	If Alias_QuestGiver != None
		giverRef = Alias_QuestGiver.GetReference()
	EndIf
	Actor playerRef = None
	If Alias_QuestPlayer != None
		playerRef = Alias_QuestPlayer.GetActorReference()
	EndIf
	If giverRef != None && playerRef != None
		RegisterForDistanceLessThanEvent(playerRef, giverRef, 250.0)
	EndIf
EndFunction

Function Fragment_Stage_0070_Item_00()
	Actor playerRef = None
	If Alias_QuestPlayer != None
		playerRef = Alias_QuestPlayer.GetActorReference()
	EndIf
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf

	If playerRef != None && Honey != None && playerRef.GetItemCount(Honey) >= 10
		playerRef.RemoveItem(Honey, 10, True)
	EndIf

	If GoodbyeScene != None
		GoodbyeScene.Start()
		Int waited = 0
		While GoodbyeScene.IsPlaying() && waited < 20
			Utility.Wait(0.5)
			waited += 1
		EndWhile
	EndIf

	If !IsStageDone(90)
		SetStage(90)
	EndIf
EndFunction

Function Fragment_Stage_0090_Item_00()
	CompleteAllObjectives()

	Actor playerRef = None
	If Alias_QuestPlayer != None
		playerRef = Alias_QuestPlayer.GetActorReference()
	EndIf
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None && FFZ13_Brew_Completed != None
		playerRef.SetValue(FFZ13_Brew_Completed, 1.0)
	EndIf

	StopWatchingQuestGiver()

	; Stage 90 carries a second log entry that the conversion appended to pay the
	; currency reward (B21:CurrencyQuestRewards RewardStageItems = 1). Yield
	; before stopping so that entry is processed while the quest is still running.
	Utility.Wait(1.0)
	Stop()
EndFunction

Function WatchQuestGiver()
	If Alias_QuestGiver == None
		Return
	EndIf
	ObjectReference giverRef = Alias_QuestGiver.GetReference()
	If giverRef != None
		RegisterForRemoteEvent(giverRef, "OnActivate")
	EndIf
EndFunction

Function StopWatchingQuestGiver()
	ObjectReference giverRef = None
	If Alias_QuestGiver != None
		giverRef = Alias_QuestGiver.GetReference()
	EndIf
	If giverRef == None
		Return
	EndIf
	UnregisterForRemoteEvent(giverRef, "OnActivate")
	Actor playerRef = None
	If Alias_QuestPlayer != None
		playerRef = Alias_QuestPlayer.GetActorReference()
	EndIf
	If playerRef != None
		UnregisterForDistanceEvents(playerRef, giverRef)
	EndIf
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActivator)
	If Alias_QuestGiver != None && akActivator == Game.GetPlayer() && akSender == Alias_QuestGiver.GetReference()
		ReachQuestGiver()
	EndIf
EndEvent

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
	ReachQuestGiver()
EndEvent

; FO76 set stage 70 from the turn-in scene that starts when the player activates
; Sweetwater carrying the honey. That scene lost its dialogue in conversion, so
; activating or reaching him stands in for it.
Function ReachQuestGiver()
	If !IsRunning() || !IsStageDone(50) || IsStageDone(70)
		Return
	EndIf
	Actor playerRef = None
	If Alias_QuestPlayer != None
		playerRef = Alias_QuestPlayer.GetActorReference()
	EndIf
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None && Honey != None && playerRef.GetItemCount(Honey) >= 10
		SetStage(70)
	EndIf
EndFunction
