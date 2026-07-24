Event OnInit()
    myWorkshop = Self.GetLinkedRef()
EndEvent

Event OnLoad()
    If myWorkshop == None
        myWorkshop = Self.GetLinkedRef()
    EndIf
EndEvent
