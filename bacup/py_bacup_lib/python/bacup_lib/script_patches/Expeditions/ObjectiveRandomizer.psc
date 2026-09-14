Event OnQuestInit()
	ApplyDeterministicSelections()
EndEvent

Function ApplyDeterministicSelections()
	SelectObjectiveForPhase(1)
	SelectObjectiveForPhase(2)
	SelectObjectiveForPhase(3)
EndFunction

Int Function SelectObjectiveForPhase(Int aiPhase)
	Int selectedStage = GetSelectedObjectiveStage(aiPhase)
	If selectedStage >= 0
		Return selectedStage
	EndIf

	Int requestedSelection = GetRequestedSelection(aiPhase)
	If requestedSelection >= 1 && requestedSelection <= 3 && IsCandidateEnabled(aiPhase, requestedSelection)
		selectedStage = (aiPhase * 10) + requestedSelection
	ElseIf IsCandidateEnabled(aiPhase, 1)
		selectedStage = (aiPhase * 10) + 1
	ElseIf IsCandidateEnabled(aiPhase, 2)
		selectedStage = (aiPhase * 10) + 2
	ElseIf IsCandidateEnabled(aiPhase, 3)
		selectedStage = (aiPhase * 10) + 3
	EndIf

	If selectedStage >= 0
		SetStage(selectedStage)
	EndIf
	Return selectedStage
EndFunction

Int Function GetSelectedObjectiveStage(Int aiPhase)
	Int firstStage = (aiPhase * 10) + 1
	If IsStageDone(firstStage)
		Return firstStage
	ElseIf IsStageDone(firstStage + 1)
		Return firstStage + 1
	ElseIf IsStageDone(firstStage + 2)
		Return firstStage + 2
	EndIf
	Return -1
EndFunction

Int Function GetRequestedSelection(Int aiPhase)
	If aiPhase == 1 && XPD_ObjMod_Debug_RNG_Obj_A != None
		Return XPD_ObjMod_Debug_RNG_Obj_A.GetValue() as Int
	ElseIf aiPhase == 2 && XPD_ObjMod_Debug_RNG_Obj_B != None
		Return XPD_ObjMod_Debug_RNG_Obj_B.GetValue() as Int
	ElseIf aiPhase == 3 && XPD_ObjMod_Debug_RNG_Obj_C != None
		Return XPD_ObjMod_Debug_RNG_Obj_C.GetValue() as Int
	EndIf
	Return 0
EndFunction

Bool Function IsCandidateEnabled(Int aiPhase, Int aiSelection)
	Int candidateStage = (aiPhase * 10) + aiSelection
	If IsStageExcluded(candidateStage)
		Return False
	EndIf

	GlobalVariable enableGlobal = None
	If aiPhase == 1
		If aiSelection == 1
			enableGlobal = XPD_ObjMod_Enable_Obj_A01
		ElseIf aiSelection == 2
			enableGlobal = XPD_ObjMod_Enable_Obj_A02
		ElseIf aiSelection == 3
			enableGlobal = XPD_ObjMod_Enable_Obj_A03
		EndIf
	ElseIf aiPhase == 2
		If aiSelection == 1
			enableGlobal = XPD_ObjMod_Enable_Obj_B01
		ElseIf aiSelection == 2
			enableGlobal = XPD_ObjMod_Enable_Obj_B02
		ElseIf aiSelection == 3
			enableGlobal = XPD_ObjMod_Enable_Obj_B03
		EndIf
	ElseIf aiPhase == 3
		If aiSelection == 1
			enableGlobal = XPD_ObjMod_Enable_Obj_C01
		ElseIf aiSelection == 2
			enableGlobal = XPD_ObjMod_Enable_Obj_C02
		ElseIf aiSelection == 3
			enableGlobal = XPD_ObjMod_Enable_Obj_C03
		EndIf
	EndIf

	Return enableGlobal == None || enableGlobal.GetValue() > 0.0
EndFunction

Bool Function IsStageExcluded(Int aiCandidateStage)
	Int index = 0
	While ConditionalExclusions != None && index < ConditionalExclusions.Length
		ConditionalExclusionsData exclusion = ConditionalExclusions[index]
		If exclusion.Exclude_ObjStage == aiCandidateStage && IsStageDone(exclusion.Conditional_ObjStage)
			Return True
		EndIf
		index += 1
	EndWhile
	Return False
EndFunction
