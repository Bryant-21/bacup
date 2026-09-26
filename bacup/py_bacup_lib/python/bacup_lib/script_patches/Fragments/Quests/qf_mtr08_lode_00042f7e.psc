MTR08_MineScript Function MineScript()
	Quest owner = Self as Quest
	Return owner as MTR08_MineScript
EndFunction

Function CompleteOpenObjective(Int aiObjective)
	If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
		SetObjectiveCompleted(aiObjective, True)
	EndIf
EndFunction

Function FailOpenObjective(Int aiObjective)
	If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
		SetObjectiveFailed(aiObjective, True)
	EndIf
EndFunction

Function RefreshEventParticipation()
	; FO76 moved its EMS spawn centre here; the single-player participation check re-evaluates instead.
	Quest owner = Self as Quest
	DefaultEventQuest eventQuest = owner as DefaultEventQuest
	If eventQuest != None
		eventQuest.RefreshParticipation()
	EndIf
EndFunction

Function Fragment_Stage_0001_Item_00()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None && MTR08_LLI_AutoMinerRepairList != None
		playerRef.AddItem(MTR08_LLI_AutoMinerRepairList, 1, False)
	EndIf
EndFunction

Function Fragment_Stage_0002_Item_00()
	MTR08_MineScript mine = MineScript()
	If mine != None
		mine.StartAllWavesForDebug()
	EndIf
EndFunction

Function Fragment_Stage_0005_Item_00()
	SetObjectiveDisplayed(5, True, True)
EndFunction

Function Fragment_Stage_0006_Item_00()
	CompleteOpenObjective(5)
	If IsStageDone(12) || IsStageDone(70)
		Return
	EndIf
	SetObjectiveDisplayed(7, True, True)
	If MTR08_Lode_StartButtonSFX != None
		MTR08_Lode_StartButtonSFX.Start()
	EndIf
	MTR08_MineScript mine = MineScript()
	If mine != None
		mine.BeginInitialSpawn()
	EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
	MTR08_MineScript mine = MineScript()
	If mine != None
		mine.StartInitialWave(1)
	EndIf
EndFunction

Function Fragment_Stage_0011_Item_00()
	MTR08_MineScript mine = MineScript()
	If mine != None
		mine.StartInitialWave(2)
	EndIf
EndFunction

Function Fragment_Stage_0012_Item_00()
	CompleteOpenObjective(5)
	CompleteOpenObjective(7)
	If IsStageDone(70)
		Return
	EndIf
	SetObjectiveDisplayed(10, True, True)
	MTR08_MineScript mine = MineScript()
	If mine != None
		mine.OpenMine()
	EndIf
	If !IsStageDone(14)
		SetStage(14)
	EndIf
EndFunction

Function Fragment_Stage_0014_Item_00()
	RefreshEventParticipation()
EndFunction

Function Fragment_Stage_0020_Item_00()
	MTR08_MineScript mine = MineScript()
	If mine != None
		mine.ScheduleEndlessWave(2)
	EndIf
EndFunction

Function Fragment_Stage_0021_Item_00()
	MTR08_MineScript mine = MineScript()
	If mine != None
		mine.ScheduleEndlessWave(1)
	EndIf
EndFunction

Function Fragment_Stage_0022_Item_00()
	MTR08_MineScript mine = MineScript()
	If mine != None
		mine.ScheduleEndlessWave(0)
	EndIf
EndFunction

Function Fragment_Stage_0070_Item_00()
	CompleteOpenObjective(10)
	MTR08_MineScript mine = MineScript()
	If mine != None
		mine.BeginCollapse()
	EndIf
	If !IsStageDone(100)
		SetObjectiveDisplayed(70, True, True)
	EndIf
	If !IsStageDone(71)
		SetStage(71)
	EndIf
	If !IsStageDone(75)
		SetStage(75)
	EndIf
EndFunction

Function Fragment_Stage_0071_Item_00()
	RefreshEventParticipation()
EndFunction

Function Fragment_Stage_0075_Item_00()
	Actor playerRef = Game.GetPlayer()
	If playerRef == None
		Return
	EndIf
	Quest owner = Self as Quest
	DefaultEventQuest eventQuest = owner as DefaultEventQuest
	If eventQuest != None && !eventQuest.IsPlayerParticipating()
		Return
	EndIf
	If ParticipationKeyword != None
		playerRef.AddKeyword(ParticipationKeyword)
	EndIf
	If MiscQuestKeyword != None
		MiscQuestKeyword.SendStoryEvent(None, playerRef)
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	CompleteOpenObjective(10)
	CompleteOpenObjective(70)
	MTR08_MineScript mine = MineScript()
	If mine != None
		mine.CompleteCollapse()
	EndIf
EndFunction

Function Fragment_Stage_9990_Item_00()
	FailOpenObjective(5)
	FailOpenObjective(7)
	Stop()
EndFunction

Function Fragment_Stage_10000_Item_00()
	MTR08_MineScript mine = MineScript()
	If mine != None
		mine.ShutdownMine()
	EndIf
EndFunction
