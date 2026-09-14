; Marker visibility is decided entirely from the player's bound actor values:
; a prerequisite value must be met, a blocking value must not be met, the Crane
; bad ending suppresses the marker, and a body-choice marker only shows for its
; own BodyIndex. Re-evaluation is rate limited by AppearanceFrequency.
Function EvaluateMarkerCriteria()
	If TargetPlayer == None
		TargetPlayer = Game.GetPlayer()
	EndIf
	If TargetPlayer == None
		Return
	EndIf

	If AllowStateChangeValue != None && TargetPlayer.GetValue(AllowStateChangeValue) <= 0.0
		Return
	EndIf

	If AppearanceFrequency != None && StateUpdateTimestampValue != None
		Float nowDays = Utility.GetCurrentGameTime()
		Float offsetDays = 0.0
		If TimeStampOffSet != None
			offsetDays = TimeStampOffSet.GetValue()
		EndIf
		Float stampDays = TargetPlayer.GetValue(StateUpdateTimestampValue)
		If stampDays > 0.0 && (nowDays - stampDays) < (AppearanceFrequency.GetValue() + offsetDays)
			Return
		EndIf
		TargetPlayer.SetValue(StateUpdateTimestampValue, nowDays)
	EndIf

	Bool bShow = True
	If PreReqAV != None && TargetPlayer.GetValue(PreReqAV) < PreReqAVValue
		bShow = False
	EndIf
	If bShow && BlockingValue != None && TargetPlayer.GetValue(BlockingValue) >= BlockingValueValue
		bShow = False
	EndIf
	If bShow && W05_MQ_004P_Crane_BadEnding != None && TargetPlayer.GetValue(W05_MQ_004P_Crane_BadEnding) > 0.0
		bShow = False
	EndIf
	If bShow && BodyIndex >= 0 && W05_MQ_003P_Muscle_PollyBodyChoiceIndex != None
		If (TargetPlayer.GetValue(W05_MQ_003P_Muscle_PollyBodyChoiceIndex) as Int) != BodyIndex
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

Event OnCellLoad()
	Self.EvaluateMarkerCriteria()
EndEvent

Event OnLoad()
	Self.EvaluateMarkerCriteria()
EndEvent
