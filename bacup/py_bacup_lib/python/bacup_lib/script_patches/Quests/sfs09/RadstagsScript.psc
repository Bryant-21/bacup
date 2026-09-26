Event OnAliasInit()
    OwningQuest = GetOwningQuest()
    QS = OwningQuest as Quests:sfs09:habitatquestscript
EndEvent

; A sickly radstag killed while the troughs are open drops fragrant venison, sometimes two.
Event OnDying(ObjectReference akSenderRef, Actor akKiller)
    If akSenderRef == None || SFS09_Habitat_Venison == None
        Return
    EndIf
    If QS == None
        OwningQuest = GetOwningQuest()
        QS = OwningQuest as Quests:sfs09:habitatquestscript
    EndIf
    If QS == None || !QS.IsTroughPhaseActive()
        Return
    EndIf
    Int venisonCount = 1
    If Utility.RandomFloat(0.0, 1.0) < QS.SecondVenisonChance
        venisonCount = 2
    EndIf
    akSenderRef.AddItem(SFS09_Habitat_Venison, venisonCount, True)
EndEvent
