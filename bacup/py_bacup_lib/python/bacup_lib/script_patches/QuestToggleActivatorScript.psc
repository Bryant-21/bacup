Function ApplyLocalQuestState()
    If ShouldOpenWhenActive && ActiveKeyword != None
        SetOpen(HasKeyword(ActiveKeyword))
    EndIf

    If isOpen
        BlockActivation(False, False)
        GoToState("open")
    Else
        BlockActivation(ShouldBlockActivationWhenClosed, ShouldHideActivateTextWhenClosed)
        GoToState("closed")
    EndIf
EndFunction

Event OnLoad()
    SetDefaultState()
    ApplyLocalQuestState()
EndEvent

Event OnReset()
    SetDefaultState()
    ApplyLocalQuestState()
EndEvent

Event OnActivate(ObjectReference akActionRef)
    Bool inactive = ShouldOpenWhenActive && ActiveKeyword != None && !HasKeyword(ActiveKeyword)
    If inactive || (GetState() == "closed" && ShouldBlockActivationWhenClosed)
        PlayInactiveSound()
        If QuestToggleActivatorInactiveMessage != None
            QuestToggleActivatorInactiveMessage.Show()
        EndIf
        Return
    EndIf

    SetOpen(!isOpen)
    ApplyLocalQuestState()
EndEvent
