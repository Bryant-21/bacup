Event OnAliasInit()
    OwningQuest = GetOwningQuest()
    MasterQuest = SFS09_Habitat_Master
    MasterScript = SFS09_Habitat_Master as Quests:sfs09:arktosmasterscript
    DefaultQuestEncounterWaveScript waves = OwningQuest as DefaultQuestEncounterWaveScript
    If waves != None
        RegisterForCustomEvent(waves, "FirstSubwaveSpawned")
    EndIf
EndEvent

Event OnAliasShutdown()
    UnregisterForAllEvents()
EndEvent

Event DefaultQuestEncounterWaveScript.FirstSubwaveSpawned(DefaultQuestEncounterWaveScript akSender, Var[] akArgs)
    BindFriendlyCreature()
EndEvent

; The habitat's summoned creature becomes the named objective creature and sides with the players.
; While ARIC-4 still runs the lab its creatures flee; after Quercus' takeover they fight.
Function BindFriendlyCreature()
    If CreatureAlias == None || CreatureAlias.GetReference() != None
        Return
    EndIf
    Actor creature = None
    Int index = 0
    While creature == None && index < GetCount()
        Actor candidate = GetAt(index) as Actor
        If candidate != None && !candidate.IsDead()
            creature = candidate
        EndIf
        index += 1
    EndWhile
    If creature == None
        Return
    EndIf
    CreatureAlias.ForceRefTo(creature)
    If MasterScript == None
        MasterScript = SFS09_Habitat_Master as Quests:sfs09:arktosmasterscript
    EndIf
    If MasterScript != None && MasterScript.SFS09_Habitat_FriendlyCreatureFaction != None
        creature.RemoveFromAllFactions()
        creature.AddToFaction(MasterScript.SFS09_Habitat_FriendlyCreatureFaction)
    EndIf
    creature.StopCombat()
    If SFS09_Habitat_CowardlyConfidenceSpell != None && (SFS09_Habitat_MainframePower == None || SFS09_Habitat_MainframePower.GetValue() >= 1.0)
        creature.AddSpell(SFS09_Habitat_CowardlyConfidenceSpell, False)
    EndIf
    creature.EvaluatePackage()
EndFunction
