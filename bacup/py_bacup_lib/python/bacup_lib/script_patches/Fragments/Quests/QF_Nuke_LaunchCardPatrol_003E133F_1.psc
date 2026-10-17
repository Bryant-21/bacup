Function Fragment_Stage_0100_Item_00()
	If Nuke_LaunchCardPatrolCountdownScene != None && !Nuke_LaunchCardPatrolCountdownScene.IsPlaying()
		Nuke_LaunchCardPatrolCountdownScene.Start()
	EndIf
	StartTimer(30.0, 100)
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID == 100 && IsRunning() && !IsStageDone(200)
		SetStage(200)
	EndIf
EndEvent

Function Fragment_Stage_0200_Item_00()
	ObjectReference crateRef = LaunchCardCrate.GetReference()
	Nuke_LaunchCardPatrolScript patrol = (Self as Quest) as Nuke_LaunchCardPatrolScript
	If crateRef != None && patrol != None && patrol.ExplosionFatMan != None
		crateRef.PlaceAtMe(patrol.ExplosionFatMan)
		crateRef.Disable()
	EndIf
	Stop()
EndFunction
