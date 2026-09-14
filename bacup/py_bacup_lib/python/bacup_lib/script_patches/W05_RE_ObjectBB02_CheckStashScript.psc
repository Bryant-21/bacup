Event OnAliasInit()
    Actor kActor = GetActorReference()
    If kActor != None
        RegisterForRemoteEvent(kActor, "OnPackageEnd")
    EndIf
EndEvent

Event Actor.OnPackageEnd(Actor akSender, Package akOldPackage)
    Quest kQuest = GetOwningQuest()
    If akSender == None || kQuest == None || akOldPackage == None
        Return
    EndIf

    If akOldPackage == W05_RE_ObjectBB02_BadGuyCharge
        kQuest.SetStage(110)
    ElseIf akOldPackage == W05_RE_ObjectBB02_BadGuyJogHappy
        kQuest.SetStage(210)
    ElseIf akOldPackage == W05_RE_ObjectBB02_BadGuyAngry
        kQuest.SetStage(260)
    ElseIf akOldPackage == W05_RE_ObjectBB02_SearchForPlayer
        kQuest.SetStage(270)
    EndIf
EndEvent
