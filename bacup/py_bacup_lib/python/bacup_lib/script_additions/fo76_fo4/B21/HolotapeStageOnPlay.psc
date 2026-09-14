Scriptname B21:HolotapeStageOnPlay Extends ObjectReference

Quest Property TargetQuest Auto Const
Int Property PrereqStage Auto Const
Int Property StageToSet Auto Const

Event OnHolotapePlay(ObjectReference akTerminalRef)
	If TargetQuest == None || !TargetQuest.IsRunning()
		Return
	EndIf
	If !TargetQuest.IsStageDone(PrereqStage) || TargetQuest.IsStageDone(StageToSet)
		Return
	EndIf
	TargetQuest.SetStage(StageToSet)
EndEvent
