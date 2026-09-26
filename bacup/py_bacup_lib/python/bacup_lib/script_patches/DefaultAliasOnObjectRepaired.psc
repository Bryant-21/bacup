; FO76 players repair a damaged destructible by paying its repair recipe: the
; COBJ whose created object is the destructible base. FO4 Papyrus cannot read a
; COBJ and cannot activate MSTT references, so the recipe rows are mirrored here
; and the repair runs when the player reaches the damaged reference.

Bool Function NeedsRepair(ObjectReference akTarget) Global
    If akTarget == None
        Return False
    EndIf
    If akTarget.IsDestroyed() || akTarget.GetCurrentDestructionStage() > 0
        Return True
    EndIf
    Actor targetActor = akTarget as Actor
    Return targetActor != None && (targetActor.IsDead() || targetActor.IsBleedingOut())
EndFunction

Int Function AddRepairComponent(Component[] akComponents, Int[] aiCounts, Int aiRow, Int aiComponentID, Int aiCount) Global
    If aiRow < 0 || aiRow >= akComponents.Length || aiRow >= aiCounts.Length
        Return aiRow
    EndIf
    akComponents[aiRow] = Game.GetFormFromFile(aiComponentID, "Fallout4.esm") as Component
    aiCounts[aiRow] = aiCount
    Return aiRow + 1
EndFunction

Bool Function IsSeventySixForm(Form akForm, Int aiObjectID) Global
    Return akForm != None && akForm == Game.GetFormFromFile(aiObjectID, "SeventySix.esm")
EndFunction

; Returns the number of rows written; 0 means no FO76 repair recipe is known.
Int Function GetRepairCost(Form akBase, Component[] akComponents, Int[] aiCounts) Global
    If akBase == None || akComponents == None || aiCounts == None
        Return 0
    EndIf
    Int rows = 0
    If IsSeventySixForm(akBase, 0x00518295)
        ; FF06_Feed_co_IndSmPipe1Way01Breakable
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FABD, 3)
    ElseIf IsSeventySixForm(akBase, 0x00339D09)
        ; co_MTNS06_Uranium_Extractor
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FABD, 10)
    ElseIf IsSeventySixForm(akBase, 0x0030E9F9)
        ; co_MTNS04_Jukebox
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FABD, 2)
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FAB7, 1)
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FAA4, 2)
    ElseIf IsSeventySixForm(akBase, 0x003D76BF)
        ; repair_co_MTNM03_MeditationHub
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FABD, 3)
    ElseIf IsSeventySixForm(akBase, 0x0037A052)
        ; co_GeneratorDestructible
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FA9A, 1)
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FA9C, 2)
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FAB9, 2)
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FABD, 4)
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FAB0, 2)
    ElseIf IsSeventySixForm(akBase, 0x00583E2F)
        ; co_E05_Radiation_LvlMainTurretTripod_NonHostileRepairableCOPY0000
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FAB4, 1)
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FAB0, 1)
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FABD, 2)
    ElseIf IsSeventySixForm(akBase, 0x0023C823)
        ; TW005PickettsFortTokenDispenserRecipe
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FAB0, 1)
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FABC, 1)
    ElseIf IsSeventySixForm(akBase, 0x0052F52E)
        ; TW005BlackBearTerminalRepair
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0003D294, 1)
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FA9B, 1)
    ElseIf IsSeventySixForm(akBase, 0x00387EE1)
        ; repair_co_BoSr01_SonicGenerator
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FABD, 1)
    ElseIf IsSeventySixForm(akBase, 0x004971B9)
        ; co_CB06_ASAM_Turret_RepairRecipe
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FABD, 1)
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FA9B, 1)
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FAB0, 1)
        rows = AddRepairComponent(akComponents, aiCounts, rows, 0x0001FAB4, 1)
    EndIf
    Return rows
EndFunction

Bool Function HasRepairComponents(Actor akRepairer, Component[] akComponents, Int[] aiCounts) Global
    Int index = 0
    While akComponents != None && aiCounts != None && index < akComponents.Length && index < aiCounts.Length
        If akComponents[index] != None && aiCounts[index] > 0
            If akRepairer.GetComponentCount(akComponents[index]) < aiCounts[index]
                Return False
            EndIf
        EndIf
        index += 1
    EndWhile
    Return True
EndFunction

Bool Function TryRepairObjectWithCost(ObjectReference akTarget, Actor akRepairer, Component[] akComponents, Int[] aiCounts) Global
    If akTarget == None || akRepairer == None || akRepairer != Game.GetPlayer()
        Return False
    EndIf
    If !NeedsRepair(akTarget)
        Return False
    EndIf
    If !HasRepairComponents(akRepairer, akComponents, aiCounts)
        Debug.Notification("You don't have the components needed to repair this.")
        Return False
    EndIf
    Int index = 0
    While akComponents != None && aiCounts != None && index < akComponents.Length && index < aiCounts.Length
        If akComponents[index] != None && aiCounts[index] > 0
            akRepairer.RemoveComponents(akComponents[index], aiCounts[index], False)
        EndIf
        index += 1
    EndWhile
    ; FO4's workshop scripts rely on Repair() raising OnDestructionStageChanged
    ; back to stage 0; ClearDestruction() raises nothing.
    akTarget.Repair()
    Return True
EndFunction

Bool Function TryRepairObject(ObjectReference akTarget, Actor akRepairer) Global
    If akTarget == None
        Return False
    EndIf
    Component[] components = new Component[5]
    Int[] counts = new Int[5]
    GetRepairCost(akTarget.GetBaseObject(), components, counts)
    Return TryRepairObjectWithCost(akTarget, akRepairer, components, counts)
EndFunction

Bool Function RepairWindowOpen()
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !(owningQuest.IsRunning() || owningQuest.IsStarting())
        Return False
    EndIf
    If PrereqStage >= 0 && !owningQuest.IsStageDone(PrereqStage)
        Return False
    EndIf
    If TurnOffStage >= 0 && owningQuest.GetStage() >= TurnOffStage
        Return False
    EndIf
    Return StageToSet < 0 || !owningQuest.IsStageDone(StageToSet)
EndFunction

Function ArmRepair()
    Actor playerRef = Game.GetPlayer()
    ObjectReference target = GetReference()
    If playerRef == None || target == None
        Return
    EndIf
    If RepairWindowOpen() && NeedsRepair(target)
        RegisterForDistanceLessThanEvent(playerRef, target, 350.0)
    Else
        UnregisterForDistanceEvents(playerRef, target)
    EndIf
EndFunction

Function CompleteRepairStage()
    Actor playerRef = Game.GetPlayer()
    ObjectReference target = GetReference()
    If playerRef != None && target != None
        UnregisterForDistanceEvents(playerRef, target)
    EndIf
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || StageToSet < 0 || owningQuest.IsStageDone(StageToSet)
        Return
    EndIf
    If PrereqStage >= 0 && !owningQuest.IsStageDone(PrereqStage)
        Return
    EndIf
    If TurnOffStage >= 0 && owningQuest.GetStage() >= TurnOffStage
        Return
    EndIf
    owningQuest.SetStage(StageToSet)
EndFunction

Bool Function AttemptAliasRepair(Actor akRepairer)
    ObjectReference target = GetReference()
    If akRepairer == None || akRepairer != Game.GetPlayer() || target == None
        Return False
    EndIf
    If !RepairWindowOpen() || !NeedsRepair(target)
        Return False
    EndIf
    If !DefaultAliasOnObjectRepaired.TryRepairObject(target, akRepairer)
        Return False
    EndIf
    CompleteRepairStage()
    Return True
EndFunction

Event OnAliasInit()
    Quest owningQuest = GetOwningQuest()
    If owningQuest != None
        RegisterForRemoteEvent(owningQuest, "OnStageSet")
    EndIf
    ArmRepair()
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    ArmRepair()
EndEvent

Event OnLoad()
    ArmRepair()
EndEvent

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
    If aiCurrentStage == 0 && aiOldStage > 0
        CompleteRepairStage()
    Else
        ArmRepair()
    EndIf
EndEvent

Event OnActivate(ObjectReference akActionRef)
    AttemptAliasRepair(akActionRef as Actor)
EndEvent

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
    Actor playerRef = Game.GetPlayer()
    If AttemptAliasRepair(playerRef)
        Return
    EndIf
    ObjectReference target = GetReference()
    If playerRef != None && target != None && RepairWindowOpen() && NeedsRepair(target)
        RegisterForDistanceGreaterThanEvent(playerRef, target, 700.0)
    EndIf
EndEvent

Event OnDistanceGreaterThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
    ArmRepair()
EndEvent

Event OnAliasShutdown()
    UnregisterForAllEvents()
EndEvent
