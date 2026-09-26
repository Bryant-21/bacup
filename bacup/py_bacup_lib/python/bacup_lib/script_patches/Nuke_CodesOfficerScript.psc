Event OnAliasInit()
    RestoreOfficer()
EndEvent

Event OnDeath(Actor akKiller)
    StartTimer(Utility.RandomFloat(respawnTimeMin, respawnTimeMax), respawnTimerID)
EndEvent

Event OnItemRemoved(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    If akBaseItem == NukeCodePage
        Nuke_CodesOfficerRefScript officer = GetReference() as Nuke_CodesOfficerRefScript
        If officer != None && officer.GetItemCount(NukeCodePage) == 0
            officer.StopBeepingRMI()
        EndIf
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == respawnTimerID
        RestoreOfficer()
    EndIf
EndEvent

Function AssignLocalCodePage(Book page)
    Actor officer = GetReference() as Actor
    If officer == None || page == None
        Return
    EndIf
    B21LocalOfficerAssigned = True
    RemoveAllInventoryEventFilters()
    NukeCodePage = page
    Nuke_CodesScript codes = GetOwningQuest() as Nuke_CodesScript
    Nuke_MasterScript master = codes.Nuke_Master as Nuke_MasterScript
    If master != None
        Int silo = 0
        While silo < 3
            Int piece = 0
            While piece < 8
                Book other = master.LocalCodePage(silo, piece)
                If other != None && other != page && officer.GetItemCount(other) > 0
                    officer.RemoveItem(other, officer.GetItemCount(other), True)
                EndIf
                piece += 1
            EndWhile
            silo += 1
        EndWhile
        master.UpdateLocalCodeCycle()
    EndIf
    If officer.GetItemCount(page) == 0
        officer.AddItem(page, 1, True)
    EndIf
    AddInventoryEventFilter(NukeCodePage)
    Nuke_CodesOfficerRefScript refScript = officer as Nuke_CodesOfficerRefScript
    If refScript != None
        refScript.CodePageRemoved = False
    EndIf
EndFunction

Function RestoreOfficer()
    Actor officer = GetReference() as Actor
    If officer == None
        Return
    EndIf
    If officer.IsDead()
        If officer.Is3DLoaded()
            StartTimer(30.0, respawnTimerID)
            Return
        EndIf
        officer.Reset()
        officer.Resurrect()
        B21LocalOfficerAssigned = False
    EndIf
    If !B21LocalOfficerAssigned
        Nuke_CodesScript codes = GetOwningQuest() as Nuke_CodesScript
        If codes != None
            AssignLocalCodePage(codes.ChooseLocalOfficerPage(Self))
        EndIf
    ElseIf NukeCodePage != None
        AddInventoryEventFilter(NukeCodePage)
    EndIf
    officer.Enable()
EndFunction
