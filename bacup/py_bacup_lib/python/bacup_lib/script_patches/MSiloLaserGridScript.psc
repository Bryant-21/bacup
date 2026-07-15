Function RequestCollisionUpdate(Actor source)
    SetDefaultState()
EndFunction

State startsclosed
    Event OnLoad()
        parent.OnLoad()
    EndEvent
EndState

State closed
    Event OnLoad()
        parent.OnLoad()
    EndEvent
EndState

State startsopen
    Event OnLoad()
        parent.OnLoad()
    EndEvent
EndState

State open
    Event OnLoad()
        parent.OnLoad()
    EndEvent
EndState
