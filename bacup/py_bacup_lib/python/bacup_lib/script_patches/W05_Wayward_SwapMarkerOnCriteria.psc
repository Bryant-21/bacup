Function EvaluateMarkerCriteria()
	If TargetPlayer == None
		TargetPlayer = Game.GetPlayer()
	EndIf
	If TargetPlayer == None
		Return
	EndIf

	If AppearanceFrequency != None && (W05_MQ_004P_Crane == None || !W05_MQ_004P_Crane.IsCompleted())
		Return
	EndIf

	Bool bShow = True
	If PreReqAV != None && TargetPlayer.GetValue(PreReqAV) != PreReqAVValue
		bShow = False
	EndIf
	If bShow && BlockingValue != None && TargetPlayer.GetValue(BlockingValue) >= BlockingValueValue
		bShow = False
	EndIf
	If bShow && B21_WaywardState.BadEndingActive(TargetPlayer, W05_MQ_004P_Crane_BadEnding)
		bShow = False
	EndIf
	If bShow && AppearanceFrequency != None
		bShow = PatronAppearsThisCycle()
	EndIf
	If bShow && BodyIndex >= 1 && BodyIndex <= 3
		If W05_MQ_004P_Crane == None || !W05_MQ_004P_Crane.IsCompleted()
			bShow = False
		ElseIf W05_MQ_003P_Muscle_PollyBodyChoiceIndex == None || (TargetPlayer.GetValue(W05_MQ_003P_Muscle_PollyBodyChoiceIndex) as Int) != BodyIndex
			bShow = False
		EndIf
	EndIf

	If bShow
		If !Self.IsEnabled()
			Self.Enable(False)
		EndIf
	ElseIf !Self.IsDisabled()
		Self.Disable(False)
	EndIf
EndFunction

Bool Function PatronAppearsThisCycle()
	Float nowMinutes = B21_WaywardState.GameMinutes()
	Float cycleStamp = nowMinutes
	If StateUpdateTimestampValue != None
		cycleStamp = TargetPlayer.GetValue(StateUpdateTimestampValue)
		Float intervalMinutes = 0.0
		If TimeStampOffSet != None
			intervalMinutes = TimeStampOffSet.GetValue()
		EndIf
		If cycleStamp <= 0.0 || nowMinutes < cycleStamp || nowMinutes - cycleStamp >= intervalMinutes
			cycleStamp = nowMinutes
			TargetPlayer.SetValue(StateUpdateTimestampValue, cycleStamp)
			If AllowStateChangeValue != None
				TargetPlayer.SetValue(AllowStateChangeValue, 1.0)
			EndIf
		EndIf
	EndIf
	If !patronAppearanceRolled && AllowStateChangeValue != None
		TargetPlayer.SetValue(AllowStateChangeValue, 1.0)
	EndIf
	If AllowStateChangeValue != None && TargetPlayer.GetValue(AllowStateChangeValue) <= 0.0
		Return IsEnabled()
	EndIf
	If !patronAppearanceRolled || patronCycleStamp != cycleStamp
		patronCycleStamp = cycleStamp
		patronShouldAppear = Utility.RandomInt(1, 100) <= AppearanceFrequency.GetValue()
		patronAppearanceRolled = True
	EndIf
	Return patronShouldAppear
EndFunction

Event OnCellLoad()
	Self.EvaluateMarkerCriteria()
EndEvent

Event OnLoad()
	Self.EvaluateMarkerCriteria()
EndEvent
