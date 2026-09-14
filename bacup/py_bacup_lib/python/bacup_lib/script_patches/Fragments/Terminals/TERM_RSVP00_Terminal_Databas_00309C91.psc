Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    If RSVP00_NewsletterSpawnRef != None && RSVP01_Book_LORE_Newsletter != None
        RSVP00_NewsletterSpawnRef.PlaceAtMe(RSVP01_Book_LORE_Newsletter, 1, False, False, False)
    EndIf
    If RSVP_REF_Printer != None
        RSVP_REF_Printer.PlayAnimation("Play01")
    EndIf
EndFunction
