; Root script of Activity: Riding Shotgun (560B13). It owns the parts of the run that only its own
; bindings can reach: the scheduled merchant/guard pair, the obstacle and crate reference arrays,
; the CP02 barricade and its bomb, and the cargo gate. The stage fragments in
; Fragments:Quests:QF_E05_Caravan_00560B13 drive it.

; FO76's server picked the pair for the day from E05_Caravan_Global_Schedule. The three rows pair
; Kieran/Eugenie, Libby/Carver and Aries/Rudy, matching the actors the wiki lists for the event.
Function AssignCaravanRoles()
	Int scheduleValue = 1
	If GlobalVariable_Schedule != None
		scheduleValue = GlobalVariable_Schedule.GetValueInt()
	EndIf

	ReferenceAlias scheduledGuard = None
	ReferenceAlias scheduledMerchant = None
	Int index = 0
	While ScheduleArray != None && index < ScheduleArray.Length
		If ScheduleArray[index].GlobalVariable_Value == scheduleValue
			scheduledGuard = ScheduleArray[index].Alias_ScheduledGuard
			scheduledMerchant = ScheduleArray[index].Alias_ScheduledMerchant
		EndIf
		index += 1
	EndWhile

	If scheduledGuard == None && ScheduleArray != None && ScheduleArray.Length > 0
		scheduledGuard = ScheduleArray[0].Alias_ScheduledGuard
		scheduledMerchant = ScheduleArray[0].Alias_ScheduledMerchant
	EndIf

	AssignCaravanRole(alias_currentGuard, scheduledGuard)
	AssignCaravanRole(alias_currentMerchant, scheduledMerchant)

	; A scheduled unique that failed to create would leave the run without a guard to talk to, so
	; fall back to any other row that did produce one.
	Int fallbackRow = 0
	While ScheduleArray != None && fallbackRow < ScheduleArray.Length
		If alias_currentGuard != None && alias_currentGuard.GetReference() == None
			AssignCaravanRole(alias_currentGuard, ScheduleArray[fallbackRow].Alias_ScheduledGuard)
		EndIf
		If alias_currentMerchant != None && alias_currentMerchant.GetReference() == None
			AssignCaravanRole(alias_currentMerchant, ScheduleArray[fallbackRow].Alias_ScheduledMerchant)
		EndIf
		fallbackRow += 1
	EndWhile

	If Stage_to_startquest > 0 && !IsStageDone(Stage_to_startquest)
		SetStage(Stage_to_startquest)
	EndIf
EndFunction

Function AssignCaravanRole(ReferenceAlias akRoleAlias, ReferenceAlias akScheduledAlias)
	If akRoleAlias == None || akScheduledAlias == None
		Return
	EndIf

	ObjectReference scheduledRef = akScheduledAlias.GetReference()
	If scheduledRef == None
		Return
	EndIf

	; The scheduled uniques are created initially disabled so only the chosen pair appears.
	scheduledRef.Enable(False)
	akRoleAlias.ForceRefTo(scheduledRef)
EndFunction

Function AdvanceCaravanSchedule()
	If GlobalVariable_Schedule == None
		Return
	EndIf

	Int rowCount = 3
	If ScheduleArray != None && ScheduleArray.Length > 0
		rowCount = ScheduleArray.Length
	EndIf

	Int nextValue = GlobalVariable_Schedule.GetValueInt() + 1
	If nextValue > rowCount || nextValue < 1
		nextValue = 1
	EndIf
	GlobalVariable_Schedule.SetValue(nextValue as Float)
EndFunction

Function SetCaravanObstaclesEnabled(Bool abEnabled)
	Int index = 0
	While alias_Obstacles != None && index < alias_Obstacles.Length
		SetCaravanRefEnabled(alias_Obstacles[index], abEnabled)
		index += 1
	EndWhile
EndFunction

Function SetCaravanCratesEnabled(Bool abEnabled)
	Int index = 0
	While alias_Crates != None && index < alias_Crates.Length
		SetCaravanRefEnabled(alias_Crates[index], abEnabled)
		index += 1
	EndWhile
EndFunction

Function SetCaravanBarricadesEnabled(Bool abEnabled)
	If alias_BarricadeRefs == None
		Return
	EndIf

	Int index = 0
	While index < alias_BarricadeRefs.GetCount()
		ObjectReference barricadeRef = alias_BarricadeRefs.GetAt(index)
		If barricadeRef != None
			If abEnabled
				barricadeRef.Enable(False)
			Else
				barricadeRef.Disable(False)
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction

; Damaging the props runs E05_Caravan_Obstacle on them, which places the explosion and debris and
; then hides the reference; the barricade collection is cleared as well so nothing blocks the road.
Function DetonateCaravanBarricade()
	DamageCaravanRef(alias_BarricadeBomb)
	SetCaravanBarricadesEnabled(False)
EndFunction

Function OpenCargoGate()
	If alias_GateActivator == None
		Return
	EndIf

	ObjectReference gateRef = alias_GateActivator.GetReference()
	If gateRef == None
		Return
	EndIf
	gateRef.SetOpen(True)
EndFunction

Function SetCaravanRefEnabled(ReferenceAlias akAlias, Bool abEnabled)
	If akAlias == None
		Return
	EndIf

	ObjectReference aliasRef = akAlias.GetReference()
	If aliasRef == None
		Return
	EndIf

	If abEnabled
		aliasRef.Enable(False)
	Else
		aliasRef.Disable(False)
	EndIf
EndFunction

Function DamageCaravanRef(ReferenceAlias akAlias)
	If akAlias == None
		Return
	EndIf

	ObjectReference aliasRef = akAlias.GetReference()
	If aliasRef != None && !aliasRef.IsDestroyed()
		aliasRef.DamageObject(1000000.0)
	EndIf
EndFunction
