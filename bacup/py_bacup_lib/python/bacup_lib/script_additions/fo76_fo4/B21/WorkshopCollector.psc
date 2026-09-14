Scriptname B21:WorkshopCollector Extends ObjectReference

LeveledItem Property Produce Auto Const
GlobalVariable Property IntervalHours Auto Const
Int Property MaxStored = 10 Auto Const

Float fLastProduced = -1.0
Int iStored = 0

Event OnInit()
    fLastProduced = Utility.GetCurrentGameTime()
EndEvent

Event OnLoad()
    Accrue()
EndEvent

Event OnActivate(ObjectReference akActionRef)
    Accrue()
EndEvent

Event OnItemRemoved(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    iStored -= aiItemCount
    If iStored < 0
        iStored = 0
    EndIf
EndEvent

Function Accrue()
    If Produce == None || IntervalHours == None || MaxStored <= 0
        Return
    EndIf

    Float intervalDays = IntervalHours.GetValue() / 24.0
    If intervalDays <= 0.0
        Return
    EndIf

    Float now = Utility.GetCurrentGameTime()
    ; Unseeded (script newly attached to an existing reference) or a clock that
    ; moved backwards would otherwise bank an unbounded backlog in one step.
    If fLastProduced < 0.0 || fLastProduced > now
        fLastProduced = now
        Return
    EndIf

    Int elapsedIntervals = ((now - fLastProduced) / intervalDays) as Int
    If elapsedIntervals <= 0
        Return
    EndIf
    ; Consume every whole interval even when the container is full, so time spent
    ; at capacity does not accrue a burst that lands the moment it is emptied.
    fLastProduced += (elapsedIntervals as Float) * intervalDays

    Int room = MaxStored - iStored
    If room <= 0
        Return
    EndIf
    If elapsedIntervals > room
        elapsedIntervals = room
    EndIf
    AddItem(Produce, elapsedIntervals, true)
    iStored += elapsedIntervals
EndFunction
