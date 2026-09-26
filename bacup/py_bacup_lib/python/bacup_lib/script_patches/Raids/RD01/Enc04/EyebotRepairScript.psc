; FO76 kept only the scan VFX helpers client-side; the repair loop was server-side.
; Raids:RD01:Enc04:QuestScript spawns the eyebot and hands it a generator. The eyebot
; flies to it and, while within repair range, keeps its unkeyed linked ref pointed at
; that generator: that link is the "repairing this generator" signal whatever owns the
; generator's health reads. A destroyed (disabled) generator is rebuilt here: after
; enough RepairAmountPercentage ticks the quest re-enables it.
; ScanningVFX comes from DLCRobot.esm and is None without Automatron.

Function PlayScanningFX(ObjectReference akTarget)
	If ScanningVFX != None && akTarget != None
		ScanningVFX.Play(Self as ObjectReference, RepairTickInterval.GetValue() + 0.5, akTarget)
	EndIf
EndFunction

Function StopScanningFX()
	If ScanningVFX != None
		ScanningVFX.Stop(Self as ObjectReference)
	EndIf
EndFunction

Function BeginRepair(ObjectReference akGenerator)
	GeneratorToRepair = akGenerator
	StartTimer(0.5, TimerID)
EndFunction

Function StopRepairing()
	SetLinkedRef(None)
	StopScanningFX()
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID != TimerID || IsDead()
		Return
	EndIf
	Raids:RD01:Enc04:QuestScript encounter = OwningQuest as Raids:RD01:Enc04:QuestScript
	If encounter == None || !OwningQuest.IsRunning()
		StopRepairing()
		Return
	EndIf
	If GeneratorToRepair == None || !GeneratorToRepair.IsDisabled()
		ObjectReference destroyed = encounter.FindGeneratorNeedingRepair()
		If destroyed != None
			GeneratorToRepair = destroyed
		EndIf
	EndIf
	If GeneratorToRepair == None
		Return
	EndIf
	If GetDistance(GeneratorToRepair) > 600.0
		StopRepairing()
		PathToReference(GeneratorToRepair, 1.0)
	Else
		SetLinkedRef(GeneratorToRepair)
		PlayScanningFX(GeneratorToRepair)
		If GeneratorToRepair.IsDisabled()
			RebuildDestroyedGenerator(encounter)
		EndIf
	EndIf
	If !IsDead()
		StartTimer(RepairTickInterval.GetValue(), TimerID)
	EndIf
EndEvent

Function RebuildDestroyedGenerator(Raids:RD01:Enc04:QuestScript akEncounter)
	Float step = RepairAmountPercentage.GetValue()
	If step <= 0.0
		Return
	EndIf
	Float rebuilt = 0.0
	While rebuilt < 1.0 && GeneratorToRepair != None && !IsDead() && OwningQuest.IsRunning() && GeneratorToRepair.IsDisabled() && GetDistance(GeneratorToRepair) <= 600.0
		PlayScanningFX(GeneratorToRepair)
		Utility.Wait(RepairTickInterval.GetValue())
		rebuilt += step
	EndWhile
	If rebuilt >= 1.0 && !IsDead()
		akEncounter.RestoreGenerator(GeneratorToRepair)
	EndIf
EndFunction

Event OnDying(Actor akKiller)
	CancelTimer(TimerID)
	StopRepairing()
	GeneratorToRepair = None
EndEvent
